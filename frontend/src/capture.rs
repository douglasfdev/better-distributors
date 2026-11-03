//! "Escanear com celular": pairs the desktop app with the standalone mobile
//! page (`capture.html`) through a locally generated QR code (`qrcode` crate).
//! The write token never leaves this app except inside the QR link; the read
//! token is only used against `/api/captures` on this same origin.
use gloo_net::http::{Request, Response};
use js_sys::Date;
use leptos::prelude::*;
use qrcode::{QrCode, render::svg};
use wasm_bindgen_futures::{JsFuture, spawn_local};
use web_sys::window;

const POLL_INTERVAL_MS: i32 = 2_000;
const REQUEST_TIMEOUT_MS: i32 = 10_000;

#[derive(Clone, PartialEq)]
pub struct Session {
    id: String,
    read_token: String,
    url: String,
    expires_at: String,
}

fn sleep(ms: i32) -> JsFuture {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        if let Some(window) = window() {
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms);
        }
    });
    JsFuture::from(promise)
}

/// Small inline SVG QR code, rendered fully offline — no external QR API.
fn qr_svg(url: &str) -> Option<String> {
    let code = QrCode::new(url.as_bytes()).ok()?;
    Some(
        code.render::<svg::Color>()
            .min_dimensions(200, 200)
            .quiet_zone(true)
            .build(),
    )
}

type ApiFuture = std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<Response, gloo_net::Error>>>,
>;

fn api_error(status: u16, fallback: &'static str) -> String {
    match status {
        503 => "Captura por celular indisponível: o servidor não tem a origem HTTPS pública (CAPTURE_PUBLIC_URL) configurada. Defina-a e reinicie a API para usar o QR.".into(),
        429 => "Limite de capturas simultâneas atingido. Cancele uma captura aberta nesta tela ou aguarde alguns minutos.".into(),
        _ => fallback.into(),
    }
}

fn remaining_seconds(expires_at: &str) -> i64 {
    let parsed = Date::parse(expires_at);
    if parsed.is_nan() {
        return 0;
    }
    (((parsed - Date::now()) / 1_000.0) as i64).max(0)
}

fn format_countdown(seconds: i64) -> String {
    format!("{:02}:{:02}", seconds / 60, seconds % 60)
}

// Outcome of one status request.
async fn poll_once(
    response: Response,
    session: &Session,
    status: RwSignal<&'static str>,
    error: RwSignal<String>,
    countdown: RwSignal<i64>,
) -> PollOutcome {
    match response.status() {
        401 => PollOutcome::Expired,
        code if code != 200 => {
            error.set(api_error(code, "A leitura do status falhou. Tentando de novo…"));
            PollOutcome::Continue
        }
        _ => {
            error.set(String::new());
            countdown.set(remaining_seconds(&session.expires_at));
            let body: Result<serde_json::Value, _> = response.json().await;
            let Ok(body) = body else {
                error.set("Resposta inválida do servidor. Tentando de novo…".into());
                return PollOutcome::Continue;
            };
            match body["status"].as_str() {
                Some("waiting") => {
                    status.set("waiting");
                    PollOutcome::Continue
                }
                Some("connected") => {
                    status.set("connected");
                    PollOutcome::Continue
                }
                Some("expired" | "ended") => PollOutcome::Expired,
                Some("received") => {
                    // Suggestion only: feeds the review field for manual
                    // application. Never saved, never applied automatically.
                    PollOutcome::Received(body["text"].as_str().unwrap_or("").to_owned(), body["sequence"].as_u64().unwrap_or(0))
                }
                other => {
                    error.set(format!(
                        "Status inesperado da captura: {}",
                        other.unwrap_or("desconhecido")
                    ));
                    PollOutcome::Continue
                }
            }
        }
    }
}

enum PollOutcome {
    Continue,
    Received(String, u64),
    Expired,
}

fn poll(
    session: Session,
    my_generation: u32,
    status: RwSignal<&'static str>,
    error: RwSignal<String>,
    countdown: RwSignal<i64>,
    open: RwSignal<bool>,
    _session_rw: RwSignal<Option<Session>>,
    generation: RwSignal<u32>,
    ocr: RwSignal<String>,
    pending: RwSignal<Option<(Session, u64)>>,
) {
    spawn_local(async move {
        let mut received_sequence = 0;
        loop {
            if generation.get() != my_generation || !open.get() {
                return;
            }
            let id = session.id.clone();
            let token = session.read_token.clone();
            let request: ApiFuture = Box::pin(async move {
                Request::get(&format!("/api/captures/{id}"))
                    .header("Authorization", &format!("Bearer {token}"))
                    .send()
                    .await
            });
            // Every request is bounded by a timeout; timeouts and network
            // errors only set a transient message, polling keeps retrying.
            match futures::future::select(
                request,
                Box::pin(async { let _ = sleep(REQUEST_TIMEOUT_MS).await; }),
            )
            .await
            {
                futures::future::Either::Left((Ok(response), _)) => {
                    if generation.get() != my_generation || !open.get() {
                        return;
                    }
                    match poll_once(response, &session, status, error, countdown).await {
                        PollOutcome::Continue => {}
                        PollOutcome::Expired => {
                            // Stop polling; show the expired state and keep the
                            // session for manual cancel/regenerate.
                            status.set("expired");
                            pending.set(None);
                            generation.update(|g| *g += 1);
                            return;
                        }
                        PollOutcome::Received(text, sequence) => {
                            if sequence > received_sequence {
                                received_sequence = sequence;
                                pending.set(Some((session.clone(), sequence)));
                                ocr.set(text);
                            }
                            status.set("review");
                        }
                    }
                }
                futures::future::Either::Left((Err(_), _)) => {
                    error.set(
                        "Sem conexão com o servidor. Tentando de novo automaticamente…".into(),
                    );
                }
                futures::future::Either::Right(_) => {
                    error.set(
                        "A resposta do servidor demorou. Tentando de novo automaticamente…".into(),
                    );
                }
            }
            // Sleep in slices so cancel/close/regenerate take effect at once.
            let _ = sleep(POLL_INTERVAL_MS).await;
            countdown.set(remaining_seconds(&session.expires_at));
        }
    });
}

fn start_capture(
    session: RwSignal<Option<Session>>,
    status: RwSignal<&'static str>,
    error: RwSignal<String>,
    generation: RwSignal<u32>,
    open: RwSignal<bool>,
    countdown: RwSignal<i64>,
    ocr: RwSignal<String>,
    pending: RwSignal<Option<(Session, u64)>>,
) {
    spawn_local(async move {
        // Invalidate any previous polling loop before touching signals.
        generation.update(|g| *g += 1);
        let my_generation = generation.get();
        session.set(None);
        error.set(String::new());
        countdown.set(0);
        status.set("creating");
        open.set(true);
        match Request::post("/api/captures").send().await {
            Ok(response) if response.ok() => {
                let body: Result<serde_json::Value, _> = response.json().await;
                if generation.get() != my_generation {
                    return;
                }
                match body {
                    Ok(body) => {
                        let created = Session {
                            id: body["id"].as_str().unwrap_or_default().to_owned(),
                            read_token: body["read_token"].as_str().unwrap_or_default().to_owned(),
                            url: body["url"].as_str().unwrap_or_default().to_owned(),
                            expires_at: body["expires_at"].as_str().unwrap_or_default().to_owned(),
                        };
                        if created.id.is_empty()
                            || created.read_token.is_empty()
                            || created.url.is_empty()
                        {
                            status.set("");
                            error.set("Resposta inválida do servidor ao abrir a captura.".into());
                            return;
                        }
                        countdown.set(remaining_seconds(&created.expires_at));
                        session.set(Some(created.clone()));
                        status.set("waiting");
                        poll(
                            created,
                            my_generation,
                            status,
                            error,
                            countdown,
                            open,
                            session,
                            generation,
                            ocr,
                            pending,
                        );
                    }
                    Err(_) => {
                        status.set("");
                        error.set("Resposta inválida do servidor ao abrir a captura.".into());
                    }
                }
            }
            Ok(response) => {
                status.set("");
                let code = response.status();
                if matches!(code, 429 | 503) {
                    error.set(api_error(code, ""));
                } else {
                    let body: Option<serde_json::Value> = response.json().await.ok();
                    error.set(
                        body.and_then(|b| b["error"].as_str().map(str::to_owned)).unwrap_or_else(|| {
                            "Não foi possível iniciar a captura.".into()
                        }),
                    );
                }
            }
            Err(_) => {
                status.set("");
                error.set("Sem conexão com o servidor. Verifique se a API está ativa.".into());
            }
        }
    });
}

pub async fn acknowledge(session: Session, sequence: u64) -> Result<(), String> {
    loop {
        let response = Request::post(&format!("/api/captures/{}/ack", session.id))
            .header("Authorization", &format!("Bearer {}", session.read_token))
            .json(&serde_json::json!({"sequence": sequence})).map_err(|e| e.to_string())?
            .send().await;
        match response {
            Ok(response) if response.ok() => return Ok(()),
            Ok(response) if response.status() < 500 => return Err("Sessão encerrada ou expirada. Gere outro QR para continuar.".into()),
            _ => { let _ = sleep(POLL_INTERVAL_MS).await; }
        }
    }
}

#[component]
pub fn CapturePanel(ocr: RwSignal<String>, pending: RwSignal<Option<(Session, u64)>>) -> impl IntoView {
    let open = RwSignal::new(false);
    let session = RwSignal::new(None::<Session>);
    let status = RwSignal::new("");
    let error = RwSignal::new(String::new());
    let countdown = RwSignal::new(0i64);
    // Incremented on every lifecycle event (start, regenerate, cancel, close,
    // terminal state) so stale poll loops stop immediately — no duplicates.
    let generation = RwSignal::new(0u32);

    // Fire-and-forget cancel: only valid while the read token is live.
    fn cancel_session(session: &Session) {
        let id = session.id.clone();
        let token = session.read_token.clone();
        spawn_local(async move {
            let _ = Request::delete(&format!("/api/captures/{id}"))
                .header("Authorization", &format!("Bearer {token}"))
                .send()
                .await;
        });
    }

    // Signal captures are Copy, so these handlers stay cheap Fn closures
    // (and there is nothing shared or cloned across the view tree).
    let start_action = move || {
        start_capture(session, status, error, generation, open, countdown, ocr, pending);
    };
    let start_click = move |_| start_action();

    // The QR SVG is injected imperatively because it is trusted, local output.
    // Both reads are tracked so the effect re-runs when the ref mounts.
    let qr_container = NodeRef::<leptos::html::Div>::new();
    Effect::new(move |_| {
        let qr = session.get().as_ref().and_then(|s| qr_svg(&s.url)).unwrap_or_default();
        if let Some(container) = qr_container.get() {
            container.set_inner_html(&qr);
        }
    });

    // Regenerate: cancel the previous session, then open a fresh one.
    let regenerate_click = move |_| {
        generation.update(|g| *g += 1);
        if let Some(current) = session.get_untracked() {
            cancel_session(&current);
            session.set(None);
        }
        pending.set(None);
        status.set("");
        start_action();
    };

    // Close: stop polling at once; cancel the server session if still open.
    let close_click = move |_| {
        generation.update(|g| *g += 1);
        open.set(false);
        status.set("");
        error.set(String::new());
        if let Some(current) = session.get_untracked() {
            cancel_session(&current);
        }
        session.set(None);
        pending.set(None);
    };

    view! {
        <Show when=move || !open.get()>
            <button
                class="secondary capture-pulse"
                type="button"
                disabled=move || status.get() == "creating"
                on:click=start_click
            >
                {move || if status.get() == "creating" { "Abrindo…" } else { "📱 Escanear com celular" }}
            </button>
        </Show>
        <Show when=move || open.get()>
            <div class="capture-panel" role="region" aria-label="Captura pelo celular">
                <div class="section-heading">
                    <span class="badge">"CAPTURA PELO CELULAR"</span>
                    <button class="icon-button" type="button" aria-label="Fechar captura" on:click=close_click>"×"</button>
                </div>
                <Show when=move || status.get() == "review">
                    <p role="status">"Texto recebido. Revise e salve a loja para liberar a próxima captura no celular."</p>
                </Show>
                <Show when=move || !error.get().is_empty()>
                    <p class="capture-error" role="alert">{move || error.get()}</p>
                </Show>
                <Show
                    when=move || { session.get().is_some() && ["waiting", "connected", "expired"].contains(&status.get()) }
                >
                    <Show when=move || status.get() == "connected">
                        <p class="capture-connected" role="status">"Celular conectado. Aguardando o texto…"</p>
                    </Show>
                    <Show when=move || status.get() == "expired">
                        <p class="capture-error" role="status">"Esta captura expirou. Gere um novo QR para continuar."</p>
                    </Show>
                    <Show when=move || { status.get() == "waiting" || status.get() == "connected" }>
                        <div class="capture-qr" role="img" aria-label="QR com o link de captura" node_ref=qr_container></div>
                        <p class="capture-link-row"><span class="muted">"Ou copie o link:"</span> <code>{move || session.get().map(|s| s.url).unwrap_or_default()}</code></p>
                        <p class="muted">{move || {
                            let left = countdown.get();
                            if left <= 0 { "Expirando…".into() } else { format!("Link válido por {}.", format_countdown(left)) }
                        }}</p>
                        <div class="actions capture-actions">
                            <button class="secondary" type="button" on:click=regenerate_click>"Gerar novo QR"</button>
                            <button class="secondary" type="button" on:click=close_click>"Cancelar captura"</button>
                        </div>
                    </Show>
                    <Show when=move || status.get() == "expired">
                        <div class="actions capture-actions">
                            <button class="primary" type="button" on:click=regenerate_click>"Gerar novo QR"</button>
                            <button class="secondary" type="button" on:click=close_click>"Fechar"</button>
                        </div>
                    </Show>
                </Show>
            </div>
        </Show>
    }
}
