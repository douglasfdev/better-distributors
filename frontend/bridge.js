// Browser-only integrations. OCR processes the selected image locally; libraries and
// language models download from public CDNs. No API keys or image upload required.
// Exported functions for image selection and OCR processing.
export { chooseImage, ocrFile, ocrDataUrl };
let ocrLibrary;
function loadOcr() {
  if (window.Tesseract) return Promise.resolve(window.Tesseract);
  if (!ocrLibrary) ocrLibrary = new Promise((resolve, reject) => {
    const script = document.createElement('script');
    script.src = 'https://cdn.jsdelivr.net/npm/tesseract.js@5.1.1/dist/tesseract.min.js';
    script.crossOrigin = 'anonymous';
    const timer = setTimeout(() => fail(), 30000);
    function fail() {
      clearTimeout(timer);
      script.remove();
      ocrLibrary = undefined;
      reject(new Error('Não foi possível carregar o leitor de imagens. Verifique sua conexão e tente novamente.'));
    }
    script.onerror = fail;
    script.onload = () => {
      clearTimeout(timer);
      if (window.Tesseract) resolve(window.Tesseract);
      else fail();
    };
    document.head.append(script);
  });
  return ocrLibrary;
}

/** Simple helper returning the provided File (for compatibility) */
async function ocrFile(file) {
  if (file instanceof File) return file;
  throw new Error('Invalid file');
}

/** Convert a File/Blob to a Data URL */
async function ocrDataUrl(file) {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(reader.result);
    reader.onerror = reject;
    reader.readAsDataURL(file);
  });
}

/* --------------------------------------------------------------------------
 * Helpers for image editing – rotate, crop and simple preview UI
 * -------------------------------------------------------------------------- */

/** Load a File/Blob into an HTMLImageElement */
function loadImage(file) {
  return new Promise((resolve, reject) => {
    const url = URL.createObjectURL(file);
    const img = new Image();
    img.onload = () => {
      URL.revokeObjectURL(url);
      resolve(img);
    };
    img.onerror = (e) => {
      URL.revokeObjectURL(url);
      reject(e);
    };
    img.src = url;
  });
}

/** Rotate a image file by a given angle (in degrees, multiples of 90 for simplicity) */
export async function rotateImage(file, angleDeg) {
  const img = await loadImage(file);
  const canvas = document.createElement('canvas');
  const ctx = canvas.getContext('2d');
  const rad = (angleDeg % 360) * Math.PI / 180;
  // suportamos apenas rotações de 90° para simplificar
if (Math.abs(angleDeg % 180) === 90) {
    canvas.width = img.height;
    canvas.height = img.width;
    ctx.translate(canvas.width / 2, canvas.height / 2);
    ctx.rotate(rad);
    ctx.drawImage(img, -img.width / 2, -img.height / 2);
  } else {
    canvas.width = img.width;
    canvas.height = img.height;
    ctx.translate(canvas.width / 2, canvas.height / 2);
    ctx.rotate(rad);
    ctx.drawImage(img, -img.width / 2, -img.height / 2);
  }
  return new Promise((resolve) => {
    canvas.toBlob((blob) => {
      resolve(new File([blob], file.name, { type: blob.type }));
    }, file.type || 'image/jpeg');
  });
}

/** Crop a image file to a rectangle {x, y, w, h} (coordinates in original image space) */
export async function cropImage(file, rect) {
  const img = await loadImage(file);
  const canvas = document.createElement('canvas');
  canvas.width = rect.w;
  canvas.height = rect.h;
  const ctx = canvas.getContext('2d');
  ctx.drawImage(img, rect.x, rect.y, rect.w, rect.h, 0, 0, rect.w, rect.h);
  return new Promise((resolve) => {
    canvas.toBlob((blob) => {
      resolve(new File([blob], file.name, { type: blob.type }));
    }, file.type || 'image/jpeg');
  });
}

const MAX_IMAGE_PIXELS = 25_000_000;
const MAX_IMAGE_DIMENSION = 10_000;
function exceedsImageResolution(width, height) {
  return width * height > MAX_IMAGE_PIXELS || width > MAX_IMAGE_DIMENSION || height > MAX_IMAGE_DIMENSION;
}

function editImage(file) {
  return new Promise((resolve, reject) => {
    const previousFocus = document.activeElement;
    const overflow = document.body.style.overflow;
    const listeners = new AbortController();
    const dialog = document.createElement('dialog');
    dialog.id = 'image-editor-overlay';
    dialog.setAttribute('aria-label', 'Editar imagem');
    dialog.style.cssText = 'box-sizing:border-box;width:min(96vw,900px);max-height:96dvh;overflow:auto;border:0;border-radius:12px;padding:16px;background:#fff;color:#183e32;';
    const hint = document.createElement('p');
    hint.id = 'image-editor-hint';
    hint.textContent = 'Arraste na imagem para selecionar o recorte. Rotacionar mantém a seleção. Confirmar envia apenas a área selecionada.';
    const canvas = document.createElement('canvas');
    canvas.id = 'image-editor-canvas';
    canvas.setAttribute('aria-label', 'Selecionar recorte');
    canvas.setAttribute('aria-describedby', hint.id);
    canvas.style.cssText = 'display:block;margin:auto;max-width:100%;max-height:55dvh;width:auto;height:auto;touch-action:none;cursor:crosshair;';
    const status = document.createElement('p');
    status.setAttribute('role', 'status');
    status.textContent = 'Carregando imagem…';
    const bar = document.createElement('div');
    bar.style.cssText = 'display:flex;flex-wrap:wrap;gap:8px;justify-content:center;';
    function button(text) {
      const element = document.createElement('button');
      element.type = 'button';
      element.textContent = text;
      element.style.cssText = 'min-height:44px;padding:8px 12px;border:0;border-radius:6px;background:#183e32;color:white;font:inherit;touch-action:manipulation;';
      bar.append(element);
      return element;
    }
    const rotate = button('Rotacionar 90°');
    const reset = button('Redefinir recorte');
    const confirm = button('Confirmar');
    const cancel = button('Cancelar');
    dialog.append(hint, canvas, status, bar);
    let img;
    let angle = 0;
    let selection;
    let drag;
    let finished = false;
    let encoding = false;
    const source = document.createElement('canvas');
    const on = (element, event, handler) => element.addEventListener(event, handler, { signal: listeners.signal });
    function finish(error, result) {
      if (finished) return;
      finished = true;
      listeners.abort();
      if (drag && canvas.hasPointerCapture(drag.id)) canvas.releasePointerCapture(drag.id);
      dialog.close();
      dialog.remove();
      document.body.style.overflow = overflow;
      previousFocus?.focus({ preventScroll: true });
      if (error) reject(error); else resolve(result);
    }
    const cancelEdit = () => finish(new Error('Edição de imagem cancelada.'));
    function controls() {
      rotate.disabled = reset.disabled = !img || encoding || !!drag;
      confirm.disabled = !img || encoding || !!drag;
    }
    function draw() {
      const ctx = canvas.getContext('2d');
      canvas.width = source.width;
      canvas.height = source.height;
      ctx.drawImage(source, 0, 0);
      const { x, y, w, h } = selection;
      ctx.fillStyle = 'rgba(0,0,0,0.55)';
      ctx.beginPath();
      ctx.rect(0, 0, canvas.width, canvas.height);
      ctx.rect(x, y, w, h);
      ctx.fill('evenodd');
      ctx.strokeStyle = '#fff';
      ctx.lineWidth = Math.max(2, source.width / 300);
      ctx.strokeRect(x, y, w, h);
      status.textContent = `Recorte: ${w} × ${h} px`;
      controls();
    }
    function renderSource() {
      source.width = angle % 180 ? img.naturalHeight : img.naturalWidth;
      source.height = angle % 180 ? img.naturalWidth : img.naturalHeight;
      const ctx = source.getContext('2d');
      ctx.translate(source.width / 2, source.height / 2);
      ctx.rotate(angle * Math.PI / 180);
      ctx.drawImage(img, -img.naturalWidth / 2, -img.naturalHeight / 2);
    }
    function point(event) {
      const bounds = canvas.getBoundingClientRect();
      return {
        x: Math.round(Math.max(0, Math.min(source.width, (event.clientX - bounds.left) * source.width / bounds.width))),
        y: Math.round(Math.max(0, Math.min(source.height, (event.clientY - bounds.top) * source.height / bounds.height)))
      };
    }
    function update(event) {
      const end = point(event);
      const x = Math.min(drag.start.x, end.x);
      const y = Math.min(drag.start.y, end.y);
      const w = Math.abs(end.x - drag.start.x);
      const h = Math.abs(end.y - drag.start.y);
      selection = w && h ? { x, y, w, h } : drag.previous;
      draw();
    }
    on(canvas, 'pointerdown', event => {
      if (!img || encoding || drag || !event.isPrimary || event.button !== 0) return;
      event.preventDefault();
      drag = { id: event.pointerId, start: point(event), previous: selection };
      canvas.setPointerCapture(event.pointerId);
      controls();
    });
    on(canvas, 'pointermove', event => {
      if (drag?.id === event.pointerId) update(event);
    });
    on(canvas, 'pointerup', event => {
      if (drag?.id !== event.pointerId) return;
      update(event);
      drag = undefined;
      canvas.releasePointerCapture(event.pointerId);
      controls();
    });
    const abortDrag = event => {
      if (drag?.id !== event.pointerId) return;
      selection = drag.previous;
      drag = undefined;
      draw();
    };
    on(canvas, 'pointercancel', abortDrag);
    on(canvas, 'lostpointercapture', abortDrag);
    on(rotate, 'click', () => {
      const { x, y, w, h } = selection;
      selection = { x: source.height - y - h, y: x, w: h, h: w };
      angle = (angle + 90) % 360;
      renderSource();
      draw();
    });
    on(reset, 'click', () => {
      selection = { x: 0, y: 0, w: source.width, h: source.height };
      draw();
    });
    on(confirm, 'click', () => {
      if (encoding || !img || drag) return;
      encoding = true;
      controls();
      try {
        const output = document.createElement('canvas');
        const { x, y, w, h } = selection;
        if (exceedsImageResolution(w, h)) return finish(new Error('A imagem deve ter no máximo 25 milhões de pixels e 10000 pixels por dimensão. Escolha uma foto menor.'));
        output.width = w;
        output.height = h;
        output.getContext('2d').drawImage(source, x, y, w, h, 0, 0, w, h);
        output.toBlob(blob => {
          if (finished) return;
          if (!blob) return finish(new Error('Não foi possível preparar a imagem. Tente novamente.'));
          finish(null, new File([blob], file.name, { type: blob.type }));
        }, file.type === 'image/jpeg' ? 'image/jpeg' : 'image/png', 0.95);
      } catch (_) { finish(new Error('Não foi possível preparar a imagem. Tente novamente.')); }
    });
    on(cancel, 'click', cancelEdit);
    on(dialog, 'cancel', event => { event.preventDefault(); cancelEdit(); });
    on(dialog, 'close', cancelEdit);
    on(window, 'pagehide', cancelEdit);
    controls();
    try {
      document.body.append(dialog);
      document.body.style.overflow = 'hidden';
      dialog.showModal();
      cancel.focus();
      loadImage(file).then(image => {
        if (finished) return;
        if (exceedsImageResolution(image.naturalWidth, image.naturalHeight)) return finish(new Error('A imagem deve ter no máximo 25 milhões de pixels e 10000 pixels por dimensão. Escolha uma foto menor.'));
        img = image;
        renderSource();
        selection = { x: 0, y: 0, w: source.width, h: source.height };
        draw();
      }).catch(() => finish(new Error('Não foi possível abrir a imagem. Escolha uma foto válida.')));
    } catch (error) { finish(error); }
  });
}

async function chooseImage(gallery) {
  const file = await new Promise((resolve, reject) => {
    const input = document.createElement('input');
    const listeners = new AbortController();
    input.type = 'file';
    input.accept = 'image/*';
    if (!gallery) input.setAttribute('capture', 'environment');
    input.hidden = true;
    document.body.append(input);
    function finish(error, selected) {
      listeners.abort();
      input.remove();
      if (error) reject(error); else resolve(selected);
    }
    input.addEventListener('cancel', () => finish(new Error('Seleção de imagem cancelada.')), { signal: listeners.signal });
    input.addEventListener('change', () => {
      const selected = input.files?.[0];
      if (!selected) return finish(new Error('Seleção de imagem cancelada.'));
      finish(null, selected);
    }, { signal: listeners.signal });
    window.addEventListener('pagehide', () => finish(new Error('Seleção de imagem cancelada.')), { signal: listeners.signal });
    try { input.click(); }
    catch (_) { finish(new Error('Não foi possível abrir a câmera ou a galeria neste navegador.')); }
  });
  if (!file.type.startsWith('image/')) throw new Error('Selecione um arquivo de imagem válido.');
  if (!file.size) throw new Error('A imagem está vazia. Escolha outro arquivo.');
  if (file.size > 12 * 1024 * 1024) throw new Error('A imagem deve ter no máximo 12 MB.');
  return editImage(file);
}

// Locate functionality – provides geolocation with proper error messages.

export async function locate() {
  if (!window.isSecureContext) throw new Error('A localização exige HTTPS ou acesso por localhost.');
  if (!navigator.geolocation) throw new Error('Este navegador não oferece localização. Informe as coordenadas manualmente.');
  return new Promise((resolve, reject) => navigator.geolocation.getCurrentPosition(
    ({ coords }) => resolve(JSON.stringify({ latitude: coords.latitude, longitude: coords.longitude, accuracy: coords.accuracy })),
    (error) => reject(new Error({
      1: 'Permissão de localização negada. Autorize o acesso nas configurações do navegador.',
      2: 'Localização indisponível. Verifique o GPS e tente novamente.',
      3: 'Tempo esgotado ao buscar localização. Tente novamente em uma área aberta.'
    }[error.code] || 'Não foi possível obter sua localização.')),
    { enableHighAccuracy: true, timeout: 15000, maximumAge: 60000 }
  ));
}

// Live streams cannot be seeked. Let the user frame and confirm a decoded frame.
function captureCamera() {
  if (!window.isSecureContext || !navigator.mediaDevices?.getUserMedia) {
    return Promise.reject(new Error('Câmera indisponível. Use HTTPS e um navegador compatível ou toque em Galeria.'));
  }
  return new Promise((resolve, reject) => {
    const previousFocus = document.activeElement;
    const dialog = document.createElement('dialog');
    dialog.setAttribute('aria-label', 'Fotografar cartão');
    dialog.style.cssText = 'width:min(92vw,640px);border:0;border-radius:16px;padding:16px;';
    const video = document.createElement('video');
    video.autoplay = true;
    video.muted = true;
    video.playsInline = true;
    video.style.cssText = 'display:block;width:100%;max-height:65vh;background:#111;';
    const take = document.createElement('button');
    take.type = 'button';
    take.textContent = 'Tirar foto';
    take.disabled = true;
    const cancel = document.createElement('button');
    cancel.type = 'button';
    cancel.textContent = 'Cancelar';
    dialog.append(video, take, cancel);
    document.body.append(dialog);
    let stream;
    let finished = false;
    let timer;
    function finish(error, blob) {
      if (finished) return;
      finished = true;
      clearTimeout(timer);
      window.removeEventListener('pagehide', onLeave);
      stream?.getTracks().forEach(track => track.stop());
      video.pause();
      video.srcObject = null;
      dialog.close();
      dialog.remove();
      previousFocus?.focus();
      if (error) reject(error); else resolve(blob);
    }
    const onLeave = () => finish(new Error('Captura cancelada.'));
    cancel.onclick = onLeave;
    dialog.addEventListener('cancel', event => { event.preventDefault(); onLeave(); });
    window.addEventListener('pagehide', onLeave);
    video.addEventListener('loadeddata', () => {
      if (video.videoWidth && video.videoHeight) {
        clearTimeout(timer);
        take.disabled = false;
        take.focus();
      }
    });
    video.addEventListener('error', () => finish(new Error('Não foi possível carregar a câmera. Toque em Galeria.')));
    take.onclick = () => {
      take.disabled = true;
      try {
        const canvas = document.createElement('canvas');
        canvas.width = video.videoWidth;
        canvas.height = video.videoHeight;
        canvas.getContext('2d').drawImage(video, 0, 0);
        canvas.toBlob(blob => finish(blob ? null : new Error('Não foi possível capturar a imagem.'), blob), 'image/jpeg', 0.9);
      } catch (error) { finish(error); }
    };
    dialog.showModal();
    cancel.focus();
    timer = setTimeout(() => finish(new Error('A câmera demorou para responder. Tente novamente ou toque em Galeria.')), 30000);
    navigator.mediaDevices.getUserMedia({ video: { facingMode: { ideal: 'environment' } }, audio: false })
      .then(async acquired => {
        // Permission can resolve after cancellation; never leak that late stream.
        if (finished) { acquired.getTracks().forEach(track => track.stop()); return; }
        stream = acquired;
        stream.getTracks().forEach(track => track.addEventListener('ended', () => finish(new Error('A câmera foi desconectada.'))));
        video.srcObject = stream;
        await video.play();
      })
      .catch(() => finish(new Error('Não foi possível acessar a câmera. Verifique a permissão ou toque em Galeria.')));
  });
}

async function scanImageImpl(gallery) {
  // Quando gallery===false queremos usar a câmera nativa (getUserMedia) para evitar o seletor de arquivos.
  // Quando gallery===true usamos o seletor de arquivos tradicional.
  const file = gallery ? await chooseImage(true) : await captureCameraFile();
  const Tesseract = await loadOcr();
  let worker;
  try {
    worker = await Tesseract.createWorker('por+eng');
    const result = await worker.recognize(file);
    const text = result.data.text.trim();
    if (!text) throw new Error('Nenhum texto encontrado. Tente uma foto mais nítida e bem iluminada.');
    return text;
  } catch (error) {
    if (error?.message?.startsWith('Nenhum texto')) throw error;
    throw new Error('Não foi possível ler a imagem. Use uma foto JPG ou PNG nítida e verifique sua conexão.');
  } finally {
    if (worker) await worker.terminate();
  }
}

// Helper that returns a File object wrapping the Blob captured via getUserMedia.
async function captureCameraFile() {
  const blob = await captureCamera();
  // Cria um File para manter a mesma API que chooseImage() devolve.
  return editImage(new File([blob], 'camera.jpg', { type: blob.type || 'image/jpeg' }));
}



// Rust's JsValue::as_string reads rejected strings, not Error objects.
// gallery is an optional boolean: scanImage() and scanImage(false) use the camera-capture selector.
export async function scanImage(gallery) {
  try { return await scanImageImpl(gallery === true); } catch (error) { throw error?.message || String(error); }
}
export function showMap(storesJson) {
  try { return showMapImpl(storesJson); } catch (error) { throw error?.message || String(error); }
}

let map;
let mapElement;
let markers;
let resizeObserver;
function showMapImpl(storesJson) {
  const L = window.L;
  if (!L) throw new Error('O mapa ainda não carregou. Verifique sua conexão e tente novamente.');
  const element = document.getElementById('store-map');
  if (!element) throw new Error('Área do mapa não encontrada.');
  let stores;
  try { stores = JSON.parse(storesJson); } catch (_) { throw new Error('Dados de fornecedores inválidos para o mapa.'); }
  if (!Array.isArray(stores)) throw new Error('A lista de fornecedores do mapa é inválida.');
  if (map && mapElement !== element) {
    resizeObserver?.disconnect();
    map.remove();
    map = undefined;
  }
  if (!map) {
    mapElement = element;
    map = L.map(element, { scrollWheelZoom: false }).setView([-14.235, -51.9253], 4);
    L.tileLayer('https://{s}.tile.openstreetmap.org/{z}/{x}/{y}.png', {
      maxZoom: 19,
      attribution: '&copy; <a href="https://www.openstreetmap.org/copyright">OpenStreetMap</a> contributors'
    }).addTo(map);
    markers = L.featureGroup().addTo(map);
    if ('ResizeObserver' in window) {
      resizeObserver = new ResizeObserver(() => map?.invalidateSize());
      resizeObserver.observe(element);
    }
  }
  markers.clearLayers();
  for (const store of stores) {
    if (!store || typeof store !== 'object') continue;
    const lat = store.latitude ?? store.lat;
    const lng = store.longitude ?? store.lng;
    if (typeof lat !== 'number' || typeof lng !== 'number' || !Number.isFinite(lat) || !Number.isFinite(lng) || Math.abs(lat) > 90 || Math.abs(lng) > 180) continue;
    const popup = document.createElement('div');
    popup.className = 'map-popup';
    const title = document.createElement('strong');
    title.textContent = String(store.name || 'Fornecedor');
    popup.append(title);
    for (const value of [store.segment ?? store.category, store.address, store.phone]) {
      if (!value) continue;
      const line = document.createElement('p');
      line.textContent = String(value);
      popup.append(line);
    }
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'primary';
    button.textContent = 'Ver fornecedor';
    button.addEventListener('click', () => window.dispatchEvent(new CustomEvent('store-selected', { detail: store.id })));
    popup.append(button);
    L.marker([lat, lng], { title: String(store.name || 'Fornecedor') }).bindPopup(popup).addTo(markers);
  }
  map.invalidateSize();
  if (markers.getLayers().length) map.fitBounds(markers.getBounds(), { padding: [44, 44], maxZoom: 15 });
  else map.setView([-14.235, -51.9253], 4);
}
