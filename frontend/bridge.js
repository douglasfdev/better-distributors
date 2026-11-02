// Browser-only integrations. OCR processes the selected image locally; libraries and
// language models download from public CDNs. No API keys or image upload required.
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

// gallery=false (camera) keeps the original behavior; gallery=true opens the file picker without capture.
function chooseImage(gallery) {
  return new Promise((resolve, reject) => {
    const input = document.createElement('input');
    input.type = 'file';
    input.accept = 'image/*';
    if (!gallery) input.setAttribute('capture', 'environment');
    // Use opacity/position instead of display:none so mobile browsers allow the click
    input.style.cssText = 'position:fixed;top:0;left:0;opacity:0;width:100%;height:100%;';
    document.body.append(input);
    function cleanup() { input.remove(); }
    input.addEventListener('cancel', () => {
      cleanup();
      reject(new Error('Seleção de imagem cancelada.'));
    }, { once: true });
    input.addEventListener('change', () => {
      const file = input.files?.[0];
      cleanup();
      if (!file) return reject(new Error('Seleção de imagem cancelada.'));
      if (!file.type.startsWith('image/')) return reject(new Error('Selecione um arquivo de imagem válido.'));
      if (!file.size) return reject(new Error('A imagem está vazia. Escolha outro arquivo.'));
      if (file.size > 12 * 1024 * 1024) return reject(new Error('A imagem deve ter no máximo 12 MB.'));
      resolve(file);
    }, { once: true });
    // Must click in the same event loop as user gesture
    try {
        input.click();
    } catch (_) {
        cleanup();
        reject(new Error('Não foi possível abrir a câmera ou a galeria neste navegador.'));
    }
  });
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
  return new File([blob], 'camera.jpg', { type: blob.type || 'image/jpeg' });
}


async function locateImpl() {
  if (!window.isSecureContext) throw new Error('A localização exige HTTPS ou acesso por localhost.');
  if (!navigator.geolocation) throw new Error('Este navegador não oferece localização. Informe as coordenadas manualmente.');
  return new Promise((resolve, reject) => navigator.geolocation.getCurrentPosition(
    ({ coords }) => resolve(JSON.stringify({ latitude: coords.latitude, longitude: coords.longitude, accuracy: coords.accuracy })),
    error => reject(new Error({
      1: 'Permissão de localização negada. Autorize o acesso nas configurações do navegador.',
      2: 'Localização indisponível. Verifique o GPS e tente novamente.',
      3: 'Tempo esgotado ao buscar localização. Tente novamente em uma área aberta.'
    }[error.code] || 'Não foi possível obter sua localização.')),
    { enableHighAccuracy: true, timeout: 15000, maximumAge: 60000 }
  ));
}

// Rust's JsValue::as_string reads rejected strings, not Error objects.
// gallery is an optional boolean: scanImage() and scanImage(false) use the camera-capture selector.
export async function scanImage(gallery) {
  try { return await scanImageImpl(gallery === true); } catch (error) { throw error?.message || String(error); }
}
export async function locate() {
  try { return await locateImpl(); } catch (error) { throw error?.message || String(error); }
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
