mod capture;
use capture::CapturePanel;
use distributor_model::{Product, Store};
use gloo_net::http::Request;
use leptos::prelude::*;
use std::collections::BTreeMap;
use wasm_bindgen::{prelude::*, JsCast};
use wasm_bindgen_futures::spawn_local;

#[wasm_bindgen(module = "/bridge.js")]
extern "C" {
    #[wasm_bindgen(catch, js_name = scanImage)]
    async fn scan_image() -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = locate)]
    async fn locate() -> Result<JsValue, JsValue>;
    #[wasm_bindgen(catch, js_name = showMap)]
    fn show_map(stores: &str) -> Result<(), JsValue>;
}
type Fields = BTreeMap<String, String>;
#[derive(Clone, Default)]
struct ProductDraft {
    name: String,
    unit: String,
    price: String,
}
fn value(f: &Fields, key: &str) -> String {
    f.get(key).cloned().unwrap_or_default()
}
fn js_error(e: JsValue) -> String {
    e.as_string().unwrap_or_else(|| {
        "Não foi possível concluir. Verifique a permissão do navegador e tente novamente.".into()
    })
}
async fn load(stores: RwSignal<Vec<Store>>, loading: RwSignal<bool>, message: RwSignal<String>) {
    loading.set(true);
    match Request::get("/api/stores").send().await {
        Ok(r) if r.ok() => match r.json::<Vec<Store>>().await {
            Ok(data) => stores.set(data),
            Err(_) => message.set("Resposta inválida do servidor.".into()),
        },
        _ => message.set(
            "Não foi possível carregar as lojas. Verifique se o servidor está disponível.".into(),
        ),
    }
    loading.set(false);
}
#[component]
fn Field(
    fields: RwSignal<Fields>,
    name: &'static str,
    label: &'static str,
    #[prop(default = "text")] kind: &'static str,
    #[prop(default = "")] placeholder: &'static str,
    #[prop(default = false)] required: bool,
) -> impl IntoView {
    view! { <label class="field"><span>{label}</span><input type=kind placeholder=placeholder required=required prop:value=move || value(&fields.get(), name) on:input=move |e| fields.update(|f| { f.insert(name.into(), event_target_value(&e)); }) /></label> }
}
fn draft_from(s: &Store) -> Fields {
    let mut f = Fields::new();
    for (k, v) in [
        ("id", &s.id),
        ("name", &s.name),
        ("segment", &s.segment),
        ("kind", &s.kind),
        ("phone", &s.phone),
        ("email", &s.email),
        ("website", &s.website),
        ("address", &s.address),
        ("catalog_url", &s.catalog_url),
        ("notes", &s.notes),
    ] {
        f.insert(k.into(), v.clone());
    }
    f.insert(
        "latitude".into(),
        s.latitude.map(|v| v.to_string()).unwrap_or_default(),
    );
    f.insert(
        "longitude".into(),
        s.longitude.map(|v| v.to_string()).unwrap_or_default(),
    );
    f
}
fn make_store(f: Fields, products: Vec<ProductDraft>) -> Result<Store, String> {
    let get = |k| value(&f, k).trim().to_string();
    if get("name").is_empty() {
        return Err("Informe o nome da loja.".into());
    }
    let lat = get("latitude");
    let lon = get("longitude");
    let (latitude, longitude) = if lat.is_empty() && lon.is_empty() {
        (None, None)
    } else {
        let a = lat
            .replace(',', ".")
            .parse::<f64>()
            .map_err(|_| "Informe latitude e longitude válidas, ou deixe ambas vazias.")?;
        let b = lon
            .replace(',', ".")
            .parse::<f64>()
            .map_err(|_| "Informe latitude e longitude válidas, ou deixe ambas vazias.")?;
        if !a.is_finite()
            || !b.is_finite()
            || !(-90.0..=90.0).contains(&a)
            || !(-180.0..=180.0).contains(&b)
        {
            return Err("Coordenadas fora dos limites permitidos.".into());
        }
        (Some(a), Some(b))
    };
    let mut parsed = vec![];
    for p in products {
        if p.name.trim().is_empty() && p.unit.trim().is_empty() && p.price.trim().is_empty() {
            continue;
        }
        if p.name.trim().is_empty() {
            return Err("Informe o nome de cada produto adicionado.".into());
        }
        let price = if p.price.trim().is_empty() {
            None
        } else {
            let n = p
                .price
                .trim()
                .replace(',', ".")
                .parse::<f64>()
                .map_err(|_| "Preço inválido. Use, por exemplo, 12,90.")?;
            if !n.is_finite() || n < 0.0 {
                return Err("O preço deve ser um número positivo ou zero.".into());
            }
            Some(n)
        };
        parsed.push(Product {
            name: p.name.trim().into(),
            unit: p.unit.trim().into(),
            price,
        });
    }
    Ok(Store {
        id: get("id"),
        name: get("name"),
        segment: get("segment"),
        kind: get("kind"),
        phone: get("phone"),
        email: get("email"),
        website: get("website"),
        address: get("address"),
        latitude,
        longitude,
        catalog_url: get("catalog_url"),
        notes: get("notes"),
        products: parsed,
        updated_at: String::new(),
    })
}
#[component]
fn App() -> impl IntoView {
    let tab = RwSignal::new(0u8);
    let stores = RwSignal::new(Vec::<Store>::new());
    let loading = RwSignal::new(true);
    let busy = RwSignal::new(false);
    let message = RwSignal::new(String::new());
    let fields = RwSignal::new(Fields::new());
    let products = RwSignal::new(Vec::<ProductDraft>::new());
    let search = RwSignal::new(String::new());
    let segment = RwSignal::new(String::new());
    let selected = RwSignal::new(None::<Store>);
    let confirm = RwSignal::new(false);
    let ocr = RwSignal::new(String::new());
    spawn_local(load(stores, loading, message));
    let listener = gloo_events::EventListener::new(
        &web_sys::window().unwrap(),
        "store-selected",
        move |event| {
            if let Some(id) = event
                .dyn_ref::<web_sys::CustomEvent>()
                .and_then(|e| e.detail().as_string())
            {
                selected.set(stores.get_untracked().into_iter().find(|s| s.id == id));
                confirm.set(false);
            }
        },
    );
    let _listener = StoredValue::new_local(listener);
    let filtered = move || {
        let q = search.get().to_lowercase();
        let seg = segment.get();
        stores
            .get()
            .into_iter()
            .filter(|s| {
                (seg.is_empty() || s.segment == seg)
                    && format!(
                        "{} {} {} {} {}",
                        s.name,
                        s.segment,
                        s.address,
                        s.phone,
                        s.products
                            .iter()
                            .map(|p| p.name.as_str())
                            .collect::<Vec<_>>()
                            .join(" ")
                    )
                    .to_lowercase()
                    .contains(&q)
            })
            .collect::<Vec<_>>()
    };
    let modal_close = NodeRef::<leptos::html::Button>::new();
    Effect::new(move |_| {
        if selected.get().is_some() {
            request_animation_frame(move || {
                if let Some(button) = modal_close.get_untracked() {
                    let _ = button.focus();
                }
            });
        }
    });
    Effect::new(move |_| {
        let current = tab.get();
        let data = filtered();
        if current == 2 {
            request_animation_frame(move || {
                if tab.get_untracked() == 2 {
                    if let Err(e) = show_map(&serde_json::to_string(&data).unwrap_or_default()) {
                        message.set(js_error(e));
                    }
                }
            });
        }
    });
    let save = move |e: web_sys::SubmitEvent| {
        e.prevent_default();
        if busy.get_untracked() {
            return;
        }
        let store = match make_store(fields.get_untracked(), products.get_untracked()) {
            Ok(s) => s,
            Err(e) => {
                message.set(e);
                return;
            }
        };
        if let Err(error) = store.validate() {
            message.set(error.into());
            return;
        }
        if store.id.is_empty() {
            let name = normalize_name(&store.name);
            let phone = phone_digits(&store.phone);
            let duplicate = stores.get_untracked().iter().any(|s| {
                (!name.is_empty() && normalize_name(&s.name) == name)
                    || (!phone.is_empty() && phone_digits(&s.phone) == phone)
            });
            if duplicate && !web_sys::window().unwrap().confirm_with_message("Já existe uma loja com este nome ou telefone. Deseja cadastrar outra mesmo assim?").unwrap_or(false) { return; }
        }
        busy.set(true);
        message.set(String::new());
        spawn_local(async move {
            let request = if store.id.is_empty() {
                Request::post("/api/stores")
            } else {
                Request::put(&format!("/api/stores/{}", store.id))
            };
            match request.json(&store) {
                Ok(r) => match r.send().await {
                    Ok(r) if r.ok() => {
                        fields.set(Fields::new());
                        products.set(vec![]);
                        ocr.set(String::new());
                        tab.set(1);
                        load(stores, loading, message).await;
                        message.set("Loja salva com sucesso.".into());
                    }
                    Ok(r) => {
                        let body = r.json::<serde_json::Value>().await.ok();
                        message.set(
                            body.and_then(|b| b["error"].as_str().map(str::to_owned))
                                .unwrap_or_else(|| "Não foi possível salvar a loja.".into()),
                        );
                    }
                    Err(_) => message
                        .set("Sem conexão com o servidor. Seu formulário foi preservado.".into()),
                },
                Err(_) => message.set("Não foi possível preparar os dados.".into()),
            }
            busy.set(false);
        });
    };
    view! {
        <div class="app-shell">
            <aside class="sidebar"><a class="brand" href="#" on:click=move |e| {e.prevent_default();tab.set(0);} ><span class="brand-mark">"b."</span><span>"better"<small>"DISTRIBUTORS"</small></span></a>
                <div class="workspace-label">"SEU ESPAÇO DE NEGÓCIOS"</div>
                <nav aria-label="Navegação principal">
                    <button class="nav-item" class:active=move || tab.get()==0 on:click=move |_|tab.set(0)><span>"＋"</span>"Registrar"</button>
                    <button class="nav-item" class:active=move || tab.get()==1 on:click=move |_|tab.set(1)><span>"▦"</span>"Lojas"<span class="nav-count">{move ||stores.get().len()}</span></button>
                    <button class="nav-item" class:active=move || tab.get()==2 on:click=move |_|tab.set(2)><span>"◎"</span>"Mapa"</button>
                </nav>
                <div class="sidebar-footer"><span class="badge">"FEITO PARA CONECTAR"</span><p>"Bons fornecedores. Novas possibilidades."</p><small>"Sua rede, em um só lugar."</small></div>
            </aside>
            <main class="main-content">
                <div class="topbar"><span>"Sua rede de fornecedores"</span><span class="workspace-pill">"● Workspace pessoal"</span></div>
                <Show when=move ||!message.get().is_empty()><div class="toast" role="status"><span>{move ||message.get()}</span><button class="icon-button" aria-label="Fechar aviso" on:click=move |_|message.set(String::new())>"×"</button></div></Show>
                <Show when=move ||tab.get()==0>
                    <header class="page-header"><div><p class="eyebrow">"CRESÇA SUA REDE"</p><h1>{move ||if value(&fields.get(),"id").is_empty(){"Uma nova conexão."}else{"Editar conexão."}}</h1><p class="muted">"Guarde os detalhes de quem pode fazer seu negócio ir além."</p></div><span class="header-tag">"✦ Tudo começa com um contato"</span></header>
                    <div class="registration-layout"><div>
                    <section class="card scan-card"><div class="scan-icon">"⌁"</div><div><h3>"Do cartão para sua rede."</h3><p class="muted">"Fotografe um cartão ou escolha uma imagem. Revise os dados antes de salvar."</p></div><button class="secondary" disabled=move ||busy.get() on:click=move |_| {busy.set(true);spawn_local(async move {match scan_image().await {Ok(v)=>ocr.set(v.as_string().unwrap_or_default()),Err(e)=>message.set(js_error(e))} busy.set(false);});}>"Escanear cartão ↗"</button></section>
                    <CapturePanel ocr=ocr />
                    <Show when=move ||!ocr.get().is_empty()><section class="card"><h3>"Texto reconhecido · revise antes de aplicar"</h3><textarea aria-label="Texto reconhecido" prop:value=move ||ocr.get() on:input=move |e|ocr.set(event_target_value(&e))></textarea><button type="button" class="secondary" on:click=move |_| {
                        let text=ocr.get_untracked(); fields.update(|f| {
                            let mut suggestions=vec![];
                            if let Some(line)=text.lines().find(|l|!l.trim().is_empty()) {suggestions.push(("name",line.trim().to_string()));}
                            for (key,pattern) in [("email",r"[\w.+-]+@[\w.-]+\.[A-Za-z]{2,}"),("phone",r"(?:\+55\s*)?\(?\d{2}\)?[\s.-]*\d{4,5}[\s.-]*\d{4}")] {if let Ok(re)=regex::Regex::new(pattern) {if let Some(m)=re.find(&text){suggestions.push((key,m.as_str().into()));}}}
                            if let Some(line)=text.lines().find(|l| {let l=l.to_lowercase();["rua ","av.","avenida ","rodovia ","travessa "].iter().any(|p|l.starts_with(p))}) {suggestions.push(("address",line.trim().into()));}
                            for (k,v) in suggestions {if value(f,k).trim().is_empty(){f.insert(k.into(),v);}}
                        });message.set("Sugestões aplicadas apenas aos campos vazios. Confira e corrija os dados.".into());
                    }>"Aplicar sugestões aos campos vazios"</button></section></Show>
                    <form on:submit=save>
                        <section class="card"><div class="section-heading"><span class="step-number">"01"</span><div><h2>"Sobre a loja"</h2><p class="muted">"O essencial para reconhecer sua próxima parceria."</p></div><span class="muted">"* obrigatório"</span></div>
                        <div class="form-grid"><Field fields name="name" label="Nome da loja *" placeholder="Como a loja se chama?" required=true/><Field fields name="segment" label="Segmento" placeholder="Ex.: Alimentos, embalagens, bebidas"/>
                        <label class="field"><span>"Tipo de estabelecimento"</span><select prop:value=move ||value(&fields.get(),"kind") on:change=move |e| fields.update(|f|{f.insert("kind".into(),event_target_value(&e));})><option value="">"Selecione o tipo"</option><option>"Distribuidor"</option><option>"Atacadista"</option><option>"Fabricante"</option><option>"Varejista"</option><option>"Outro"</option></select></label>
                        <Field fields name="phone" label="Telefone / WhatsApp" kind="tel" placeholder="(11) 99999-9999"/><Field fields name="email" label="E-mail" kind="email" placeholder="contato@loja.com.br"/><Field fields name="website" label="Site" kind="url" placeholder="https://"/></div></section>
                        <section class="card"><div class="section-heading"><span class="step-number">"02"</span><div><h2>"Onde encontrar"</h2><p class="muted">"Adicione coordenadas para aparecer no mapa."</p></div></div><div class="form-grid"><div class="full-width"><Field fields name="address" label="Endereço completo" placeholder="Rua, número, bairro, cidade e estado"/></div><Field fields name="latitude" label="Latitude" placeholder="-23.5505"/><Field fields name="longitude" label="Longitude" placeholder="-46.6333"/></div><button type="button" class="secondary" disabled=move ||busy.get() on:click=move |_|{busy.set(true);spawn_local(async move {match locate().await {Ok(v)=>{if let Some(s)=v.as_string(){if let Ok(data)=serde_json::from_str::<serde_json::Value>(&s){fields.update(|f|{for k in ["latitude","longitude"] {if let Some(n)=data[k].as_f64(){f.insert(k.into(),n.to_string());}}});message.set("Sua localização foi preenchida. Confirme se corresponde à loja.".into());}}},Err(e)=>message.set(js_error(e))}busy.set(false);});}>"◎ Usar minha localização"</button></section>
                        <section class="card"><div class="section-heading"><span class="step-number">"03"</span><div><h2>"Produtos e catálogo"</h2><p class="muted">"Opcional. Anote produtos com ou sem preço."</p></div></div><Field fields name="catalog_url" label="Link do catálogo" kind="url" placeholder="https://loja.com.br/catalogo"/>
                        <For each={move ||(0..products.get().len()).collect::<Vec<_>>()} key=|i|*i children=move |i|view!{<div class="product-row form-grid"><label class="field"><span>"Produto"</span><input placeholder="Nome do produto" prop:value=move ||products.get().get(i).map(|p|p.name.clone()).unwrap_or_default() on:input=move |e|products.update(|ps|if let Some(p)=ps.get_mut(i){p.name=event_target_value(&e)})/></label><label class="field"><span>"Unidade"</span><input placeholder="kg, caixa, un." prop:value=move ||products.get().get(i).map(|p|p.unit.clone()).unwrap_or_default() on:input=move |e|products.update(|ps|if let Some(p)=ps.get_mut(i){p.unit=event_target_value(&e)})/></label><label class="field"><span>"Preço (R$) · opcional"</span><input inputmode="decimal" placeholder="Não informado" prop:value=move ||products.get().get(i).map(|p|p.price.clone()).unwrap_or_default() on:input=move |e|products.update(|ps|if let Some(p)=ps.get_mut(i){p.price=event_target_value(&e)})/></label><button class="secondary" type="button" on:click=move |_|products.update(|ps|{ps.remove(i);})>"Remover produto"</button></div>}/>
                        <button type="button" class="secondary" on:click=move |_|products.update(|ps|ps.push(ProductDraft::default()))>"＋ Adicionar produto"</button><label class="field"><span>"Observações"</span><textarea placeholder="Condições de pagamento, pedido mínimo, horários…" prop:value=move ||value(&fields.get(),"notes") on:input=move |e|fields.update(|f|{f.insert("notes".into(),event_target_value(&e));})></textarea></label></section>
                        <div class="actions form-actions"><span class="muted">"Uma boa conexão merece ser lembrada."</span><button type="button" class="secondary" on:click=move |_| {if web_sys::window().unwrap().confirm_with_message("Limpar todos os campos do formulário?").unwrap_or(false){fields.set(Fields::new());products.set(vec![]);ocr.set(String::new());}}>"Limpar"</button><button class="primary" type="submit" disabled=move ||busy.get()>{move ||if busy.get(){"Processando…"}else{"Salvar loja ↗"}}</button></div>
                    </form></div><aside class="registration-aside"><div class="tip-card"><span class="eyebrow">"CADA CONTATO CONTA"</span><div class="connection-art" aria-hidden="true">"✳"</div><h2>"Sua próxima parceria começa aqui."</h2><p>"Organize fornecedores, compare possibilidades e mantenha as boas conexões por perto."</p><hr/><p>"01 · Registre os detalhes"</p><p>"02 · Explore sua rede"</p><p>"03 · Encontre no mapa"</p></div><div class="card"><span class="badge">"DICA RÁPIDA"</span><p class="muted">"Não tem todas as informações? Tudo bem. Comece pelo nome e complete quando quiser."</p></div></aside></div>
                </Show>
                <Show when=move ||tab.get()==1>
                    <header class="page-header"><div><p class="eyebrow">"CONEXÕES QUE FICAM"</p><h1>"Sua rede de lojas."</h1><p class="muted">"Todos os seus fornecedores, organizados em um só lugar."</p></div><button class="primary" on:click=move |_|tab.set(0)>"＋ Registrar loja"</button></header>
                    <div class="stats-grid"><div class="stat-card"><span class="muted">"Lojas cadastradas"</span><strong>{move ||stores.get().len()}</strong></div><div class="stat-card"><span class="muted">"No mapa"</span><strong>{move ||stores.get().iter().filter(|s|s.latitude.is_some()&&s.longitude.is_some()).count()}</strong></div><div class="stat-card"><span class="muted">"Produtos registrados"</span><strong>{move ||stores.get().iter().map(|s|s.products.len()).sum::<usize>()}</strong></div></div>
                    <div class="toolbar"><input aria-label="Buscar lojas" placeholder="Buscar por loja, produto ou endereço…" prop:value=move ||search.get() on:input=move |e|search.set(event_target_value(&e))/><select aria-label="Filtrar segmento" prop:value=move ||segment.get() on:change=move |e|segment.set(event_target_value(&e))><option value="">"Todos os segmentos"</option>{move ||{let mut segs=stores.get().iter().map(|s|s.segment.clone()).filter(|s|!s.is_empty()).collect::<Vec<_>>();segs.sort();segs.dedup();segs.into_iter().map(|s|view!{<option value=s.clone()>{s.clone()}</option>}).collect_view()}}</select><button class="secondary" disabled=move ||loading.get() on:click=move |_|spawn_local(load(stores,loading,message))>"Atualizar"</button></div>
                    <Show when=move ||loading.get()><p role="status" class="muted">"Carregando sua rede…"</p></Show>
                    <Show when=move ||!loading.get()&&filtered().is_empty()><div class="card empty-state"><span class="empty-icon">"✳"</span><h2>"Espaço para novas conexões."</h2><p class="muted">"Nenhuma loja encontrada. Registre sua primeira loja ou ajuste os filtros."</p><button class="primary" on:click=move |_|tab.set(0)>"Registrar uma loja ↗"</button></div></Show>
                    <div class="store-grid">{move ||filtered().into_iter().map(|s|{let open=s.clone();view!{<article class="card store-card"><div class="store-card-top"><span class="store-avatar">{s.name.chars().next().unwrap_or('L').to_uppercase().to_string()}</span><span class="badge">{if s.kind.is_empty(){"Loja".into()}else{s.kind.clone()}}</span></div><h3>{s.name}</h3><p class="muted">{if s.segment.is_empty(){"Segmento não informado".into()}else{s.segment}}</p><p>{if s.address.is_empty(){"Endereço não informado".into()}else{s.address}}</p><div class="store-card-footer"><span class="muted">{format!("{} produtos",s.products.len())}</span><button class="secondary" on:click=move |_|{selected.set(Some(open.clone()));confirm.set(false);}>"Ver detalhes ↗"</button></div></article>}}).collect_view()}</div>
                </Show>
                <Show when=move ||tab.get()==2><header class="page-header"><div><p class="eyebrow">"MAIS PERTO DO QUE VOCÊ IMAGINA"</p><h1>"Conexões pelo caminho."</h1><p class="muted">"Selecione um marcador para conhecer a loja."</p></div></header><div class="toolbar"><input aria-label="Buscar lojas no mapa" placeholder="Buscar por loja, produto ou endereço…" prop:value=move ||search.get() on:input=move |e|search.set(event_target_value(&e))/><select aria-label="Filtrar segmento no mapa" prop:value=move ||segment.get() on:change=move |e|segment.set(event_target_value(&e))><option value="">"Todos os segmentos"</option>{move ||{let mut segs=stores.get().iter().map(|s|s.segment.clone()).filter(|s|!s.is_empty()).collect::<Vec<_>>();segs.sort();segs.dedup();segs.into_iter().map(|s|view!{<option value=s.clone()>{s.clone()}</option>}).collect_view()}}</select><button class="secondary" on:click=move |_|{search.set(String::new());segment.set(String::new());}>"Limpar filtros"</button></div><div class="map-layout"><div class="card"><h3>"Sua rede no mapa"</h3><p class="muted">{move ||format!("{} lojas com coordenadas nos filtros atuais. As demais continuam disponíveis na aba Lojas.",filtered().iter().filter(|s|s.latitude.is_some()&&s.longitude.is_some()).count())}</p><button class="secondary" on:click=move |_|tab.set(1)>"Ver todas as lojas"</button></div><div id="store-map" class="map-panel" style="min-height: 540px" aria-label="Mapa de lojas"></div></div></Show>
            </main>
        </div>
        {move || selected.get().map(|s| {let edit=s.clone();let delete_id=s.id.clone();let no_products=s.products.is_empty();view!{
            <div class="modal-overlay" on:click=move |_|{selected.set(None);confirm.set(false);}>
                <section class="modal" role="dialog" aria-modal="true" aria-labelledby="detail-title" tabindex="-1" on:click=|e|e.stop_propagation() on:keydown=move |e|if e.key()=="Escape"{selected.set(None);}>
                    <div class="section-heading"><span class="badge">{s.kind.clone()}</span><button node_ref=modal_close class="icon-button" aria-label="Fechar detalhes" on:click=move |_|selected.set(None)>"×"</button></div><h2 id="detail-title">{s.name.clone()}</h2><p class="muted">{s.segment.clone()}</p><div class="detail-grid"><div><small>"ENDEREÇO"</small><p>{or_dash(&s.address)}</p>{directions_url(&s).map(|url|view!{<a href=url target="_blank" rel="noopener noreferrer">"Como chegar ↗"</a>})}</div><div><small>"TELEFONE"</small><p><ContactLinks phone=s.phone.clone()/></p></div><div><small>"E-MAIL"</small><p>{if s.email.trim().is_empty() { view!{<span>"Não informado"</span>}.into_any() } else { view!{<a href=format!("mailto:{}",encode_component(s.email.trim()))>{s.email.clone()}</a>}.into_any() }}</p></div><div><small>"SITE"</small><p><SafeLink url=s.website.clone()/></p></div><div><small>"CATÁLOGO"</small><p><SafeLink url=s.catalog_url.clone()/></p></div></div><h3>"Produtos"</h3><Show when=move ||no_products><p class="muted">"Nenhum produto registrado."</p></Show><div class="product-list">{s.products.iter().map(|p|view!{<div class="product-detail"><strong>{p.name.clone()}</strong><span>{p.unit.clone()}</span><span>{p.price.map(|n|format!("R$ {:.2}",n).replace('.',",")).unwrap_or_else(||"Preço não informado".into())}</span></div>}).collect_view()}</div><h3>"Observações"</h3><p style="white-space: pre-wrap">{or_dash(&s.notes)}</p><p class="muted">{format!("Última atualização: {}",or_dash(&s.updated_at))}</p>
                    <Show when=move ||confirm.get()><p role="alert">"Excluir esta loja permanentemente? Esta ação não pode ser desfeita."</p></Show>
                    <div class="actions"><button class="secondary" disabled=move ||busy.get() on:click=move |_|{if !confirm.get_untracked(){confirm.set(true);return;}let id=delete_id.clone();busy.set(true);spawn_local(async move {match Request::delete(&format!("/api/stores/{id}")).send().await {Ok(r) if r.ok()=>{selected.set(None);confirm.set(false);load(stores,loading,message).await;message.set("Loja excluída.".into());},_=>message.set("Não foi possível excluir a loja.".into())}busy.set(false);});}>{move ||if confirm.get(){"Confirmar exclusão"}else{"Excluir loja"}}</button><button class="primary" on:click=move |_|{fields.set(draft_from(&edit));products.set(edit.products.iter().map(|p|ProductDraft{name:p.name.clone(),unit:p.unit.clone(),price:p.price.map(|n|n.to_string()).unwrap_or_default()}).collect());ocr.set(String::new());selected.set(None);tab.set(0);}>"Editar loja ↗"</button></div>
                </section>
            </div>
        }})}
    }
}
fn normalize_name(s: &str) -> String {
    s.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
}
fn phone_digits(s: &str) -> String {
    s.chars().filter(char::is_ascii_digit).collect()
}
fn encode_component(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
fn directions_url(s: &Store) -> Option<String> {
    let destination = match (s.latitude, s.longitude) {
        (Some(a), Some(b)) => format!("{a},{b}"),
        _ if !s.address.trim().is_empty() => s.address.trim().into(),
        _ => return None,
    };
    Some(format!(
        "https://www.google.com/maps/dir/?api=1&destination={}",
        encode_component(&destination)
    ))
}
#[component]
fn ContactLinks(phone: String) -> impl IntoView {
    let digits = phone_digits(&phone);
    if digits.is_empty() {
        return view! {<span>"Não informado"</span>}.into_any();
    }
    let whatsapp = if matches!(digits.len(), 10 | 11) {
        format!("55{digits}")
    } else {
        digits.clone()
    };
    view!{<span class="contact-links"><a href=format!("tel:{digits}")>{phone.clone()}</a>" · "<a href=format!("https://wa.me/{whatsapp}") target="_blank" rel="noopener noreferrer">"WhatsApp ↗"</a></span>}.into_any()
}
fn or_dash(s: &str) -> String {
    if s.is_empty() {
        "Não informado".into()
    } else {
        s.into()
    }
}
#[component]
fn SafeLink(url: String) -> impl IntoView {
    if url.starts_with("https://") || url.starts_with("http://") {
        view! {<a href=url.clone() target="_blank" rel="noopener noreferrer">{url.clone()}</a>}
            .into_any()
    } else {
        view! {<span>{or_dash(&url)}</span>}.into_any()
    }
}
fn main() {
    console_error_panic_hook::set_once();
    leptos::mount::mount_to_body(App);
}
