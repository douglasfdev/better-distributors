const { test, expect } = require('@playwright/test');

// Real Chromium MediaStream, decoder, canvas, file chooser and application bridge.
// Only the expensive/network-dependent OCR engine is substituted.
test.use({ launchOptions: { args: ['--use-fake-device-for-media-stream', '--use-fake-ui-for-media-stream'] } });
const api = process.env.CAPTURE_TEST_API || 'http://127.0.0.1:3100';
const publicUrl = process.env.CAPTURE_TEST_URL || 'http://127.0.0.1:3101';
let session;
test.beforeEach(async ({ page, request }) => {
  const response = await request.post(`${api}/api/captures`);
  expect(response.status()).toBe(201);
  session = await response.json();
  await page.addInitScript(() => {
    window.ocrCalls = [];
    window.ocrTerminated = 0;
    window.Tesseract = { createWorker: async () => ({
      recognize: async file => {
        const bitmap = await createImageBitmap(file);
        const canvas = document.createElement('canvas');
        canvas.width = bitmap.width;
        canvas.height = bitmap.height;
        const ctx = canvas.getContext('2d');
        ctx.drawImage(bitmap, 0, 0);
        const pixel = (x, y) => Array.from(ctx.getImageData(x, y, 1, 1).data);
        window.ocrCalls.push({ type: file.type, size: file.size, width: bitmap.width, height: bitmap.height,
          top: pixel(Math.floor(bitmap.width / 2), Math.floor(bitmap.height / 4)),
          bottom: pixel(Math.floor(bitmap.width / 2), Math.floor(bitmap.height * 3 / 4)) });
        bitmap.close();
        if (window.failOcr) throw new Error('OCR failure');
        return { data: { text: 'TEST OCR RESULT' } };
      },
      terminate: async () => { window.ocrTerminated++; }
    }) };
  });
  await page.goto(`${publicUrl}/capture${new URL(session.url).hash}`);
  await expect(page.locator('#state-ready')).toBeVisible();
  expect(new URL(page.url()).hash).toBe('');
});
test.afterEach(async ({ request }) => {
  if (session) await request.delete(`${api}/api/captures/${session.id}`, { headers: { authorization: `Bearer ${session.read_token}` } });
});

async function openCamera(page) {
  await page.locator('#btn-camera').click();
  await expect(page.getByRole('button', { name: 'Tirar foto' })).toBeEnabled();
  // Retain a reference for observing cleanup, without modifying the stream or UI.
  await page.evaluate(() => { window.cameraTracks = document.querySelector('video').srcObject.getTracks(); });
}
async function expectCleanup(page) {
  await expect(page.locator('dialog')).toHaveCount(0);
  expect(await page.evaluate(() => window.cameraTracks.every(t => t.readyState === 'ended'))).toBe(true);
  await expect(page.locator('#btn-camera')).toBeEnabled();
  await expect(page.locator('#btn-gallery')).toBeEnabled();
}
async function chooseFile(page, confirm = true) {
  const chooserPromise = page.waitForEvent('filechooser');
  await page.locator('#btn-gallery').click();
  const chooser = await chooserPromise;
  expect(await chooser.element().getAttribute('capture')).toBeNull();
  const png = await page.evaluate(() => {
    const canvas = document.createElement('canvas');
    canvas.width = 1000;
    canvas.height = 600;
    const ctx = canvas.getContext('2d');
    ctx.fillStyle = '#ff0000';
    ctx.fillRect(0, 0, 500, 600);
    ctx.fillStyle = '#0000ff';
    ctx.fillRect(500, 0, 500, 600);
    return canvas.toDataURL('image/png').split(',')[1];
  });
  await chooser.setFiles({ name: 'card.png', mimeType: 'image/png', buffer: Buffer.from(png, 'base64') });
  if (confirm) {
    await page.getByRole('button', { name: /Confirmar/ }).click();
    await expect(page.locator('#image-editor-overlay')).toHaveCount(0);
    await expect(page.locator('#btn-gallery')).toBeEnabled();
  }
}

test('camera previews, takes a real frame, releases tracks and submits to real session', async ({ page, request }) => {
  await expect(page.locator('video')).toHaveCount(0);
  await openCamera(page);
  expect(await page.evaluate(() => window.ocrCalls.length)).toBe(0);
  await page.getByRole('button', { name: 'Tirar foto' }).click();
  await expect(page.locator('#image-editor-canvas')).toBeVisible();
  expect(await page.evaluate(() => window.cameraTracks.every(t => t.readyState === 'ended'))).toBe(true);
  expect(await page.evaluate(() => window.ocrCalls.length)).toBe(0);
  const dimensions = await page.locator('#image-editor-canvas').evaluate(c => ({ width: c.width, height: c.height }));
  await selectCrop(page, 0.25, 0.25, 0.75, 0.75);
  await page.getByRole('button', { name: 'Rotacionar 90°' }).click();
  await page.getByRole('button', { name: 'Confirmar', exact: true }).click();
  await expect(page.locator('#text')).toHaveValue('TEST OCR RESULT');
  expect(await page.evaluate(() => ({ width: window.ocrCalls[0].width, height: window.ocrCalls[0].height })))
    .toEqual({ width: dimensions.height / 2, height: dimensions.width / 2 });
  await expectCleanup(page);
  await expect(page.locator('#scan-status')).toBeHidden();
  const [image] = await page.evaluate(() => window.ocrCalls);
  expect(image.type).toBe('image/jpeg');
  expect(image.width).toBeGreaterThan(100);
  expect(image.height).toBeGreaterThan(100);
  expect(image.size).toBeGreaterThan(100);
  expect(await page.evaluate(() => window.ocrTerminated)).toBe(1);
  await page.locator('#btn-send').click();
  await expect(page.locator('#state-done')).toBeVisible();
  const result = await request.get(`${api}/api/captures/${session.id}`, { headers: { authorization: `Bearer ${session.read_token}` } });
  expect((await result.json()).text).toBe('TEST OCR RESULT');
});

test('two captures on one phone wait for desktop save; failed save stays locked; phone ends', async ({ page, context, request }) => {
  const desktop = await context.newPage();
  desktop.on('dialog', dialog => dialog.accept());
  await desktop.goto(api);
  await desktop.getByRole('button', { name: '📱 Escanear com celular' }).click();
  const link = desktop.locator('.capture-link-row code');
  await expect(link).toContainText('#token=');
  const createdUrl = await link.textContent();
  await page.goto('about:blank');
  await page.goto(createdUrl);
  await expect(page.locator('#state-ready')).toBeVisible();
  const names = [`E2E first ${Date.now()}`, `E2E second ${Date.now()}`];
  for (const [index, name] of names.entries()) {
    await chooseFile(page);
    await page.locator('#text').fill(name);
    await page.locator('#btn-send').click();
    await expect(page.locator('#state-done')).toBeVisible();
    await expect(desktop.getByLabel('Texto reconhecido')).toHaveValue(name, { timeout: 10000 });
    await desktop.getByRole('button', { name: 'Aplicar sugestões aos campos vazios' }).click();
    if (index === 0) {
      await desktop.route('**/api/stores', route => route.request().method() === 'POST' ? route.fulfill({ status: 500, contentType: 'application/json', body: '{"error":"Falha simulada"}' }) : route.continue());
      await desktop.getByRole('button', { name: 'Salvar loja ↗' }).click();
      await expect(desktop.getByRole('button', { name: 'Salvar loja ↗' })).toBeEnabled();
      await page.waitForTimeout(2200);
      await expect(page.locator('#state-done')).toBeVisible();
      await expect(page.locator('#btn-gallery')).toBeHidden();
      await desktop.unroute('**/api/stores');
    }
    await desktop.getByRole('button', { name: 'Salvar loja ↗' }).click();
    await expect(page.locator('#state-ready')).toBeVisible({ timeout: 10000 });
    await expect(page.locator('#text')).toHaveValue('');
  }
  await page.locator('#btn-end').click();
  await expect(page.locator('#state-ended')).toBeVisible();
  await expect(page.locator('#btn-camera')).toBeHidden();
  const stores = await (await request.get(`${api}/api/stores`)).json();
  for (const name of names) {
    const store = stores.find(s => s.name === name);
    expect(store).toBeTruthy();
    await request.delete(`${api}/api/stores/${store.id}`);
  }
});

test('desktop cancellation ends phone while connected', async ({ page, request }) => {
  await request.delete(`${api}/api/captures/${session.id}`, { headers: { authorization: `Bearer ${session.read_token}` } });
  await expect(page.locator('#state-ended')).toBeVisible();
  await expect(page.locator('#btn-gallery')).toBeHidden();
});

test('camera cancel releases tracks, does not invoke OCR or open gallery, and can retry', async ({ page }) => {
  let pickers = 0;
  const errors = [];
  page.on('pageerror', error => errors.push(error.message));
  page.on('filechooser', () => pickers++);
  await page.evaluate(() => {
    const add = window.addEventListener;
    const remove = window.removeEventListener;
    const wrappers = new Map();
    window.exitListenerCalls = [];
    window.exitListenerRegistrations = [];
    window.addEventListener = function(type, listener, options) {
      if (!['keydown', 'keyup', 'keypress', 'pagehide'].includes(type) || !listener) return add.call(this, type, listener, options);
      let byType = wrappers.get(listener);
      if (!byType) wrappers.set(listener, byType = new Map());
      if (!byType.has(type)) byType.set(type, function(event) {
        window.exitListenerCalls.push(type);
        if (typeof listener === 'function') return listener.call(this, event);
        return listener.handleEvent(event);
      });
      window.exitListenerRegistrations.push(type);
      return add.call(this, type, byType.get(type), options);
    };
    window.removeEventListener = function(type, listener, options) {
      return remove.call(this, type, wrappers.get(listener)?.get(type) || listener, options);
    };
  });
  await openCamera(page);
  await page.getByRole('button', { name: 'Cancelar', exact: true }).click();
  await expectCleanup(page);
  await expect(page.locator('#scan-status')).toHaveText('Captura cancelada.');
  expect(await page.evaluate(() => window.exitListenerRegistrations)).toContain('pagehide');
  const calls = await page.evaluate(() => window.exitListenerCalls);
  const status = await page.locator('#scan-status').evaluate(element => element.outerHTML);
  await page.evaluate(() => {
    window.statusChanges = 0;
    new MutationObserver(records => { window.statusChanges += records.length; })
      .observe(document.querySelector('#scan-status'), { attributes: true, childList: true, characterData: true, subtree: true });
  });
  await page.keyboard.press('Escape');
  await page.evaluate(() => window.dispatchEvent(new Event('pagehide')));
  expect(await page.evaluate(() => window.exitListenerCalls)).toEqual(calls);
  expect(await page.locator('#scan-status').evaluate(element => element.outerHTML)).toBe(status);
  expect(await page.evaluate(() => window.statusChanges)).toBe(0);
  expect(errors).toEqual([]);
  await expect(page.locator('dialog')).toHaveCount(0);
  expect(await page.evaluate(() => window.ocrCalls.length)).toBe(0);
  expect(pickers).toBe(0);
  await openCamera(page);
  await page.keyboard.press('Escape');
  await expectCleanup(page);
});

test('gallery opens native picker and passes selected image to OCR', async ({ page }) => {
  await chooseFile(page);
  await expect(page.locator('#text')).toHaveValue('TEST OCR RESULT');
  await expect(page.locator('input[type=file]')).toHaveCount(0);
  await expect(page.locator('#scan-status')).toBeHidden();
  expect(await page.evaluate(() => window.ocrCalls[0].type)).toBe('image/png');
});

async function selectCrop(page, x1, y1, x2, y2) {
  const bounds = await page.locator('#image-editor-canvas').boundingBox();
  await page.mouse.move(bounds.x + bounds.width * x1, bounds.y + bounds.height * y1);
  await page.mouse.down();
  await page.mouse.move(bounds.x + bounds.width * x2, bounds.y + bounds.height * y2, { steps: 5 });
  await page.mouse.up();
}

for (const rotateFirst of [false, true]) {
  test(`scaled gallery crop preserves pixels, rotate first: ${rotateFirst}`, async ({ page }) => {
    await page.setViewportSize({ width: 390, height: 844 });
    await chooseFile(page, false);
    await expect(page.getByRole('button', { name: 'Confirmar', exact: true })).toBeEnabled();
    expect(await page.evaluate(() => window.ocrCalls.length)).toBe(0);
    if (rotateFirst) await page.getByRole('button', { name: 'Rotacionar 90°' }).click();
    await selectCrop(page, 0.75, 0.75, 0.25, 0.25);
    if (!rotateFirst) await page.getByRole('button', { name: 'Rotacionar 90°' }).click();
    await page.getByRole('button', { name: 'Confirmar', exact: true }).click();
    await expect(page.locator('#text')).toHaveValue('TEST OCR RESULT');
    const [image] = await page.evaluate(() => window.ocrCalls);
    expect(image.width).toBeCloseTo(300, -1);
    expect(image.height).toBeCloseTo(500, -1);
    expect(image.top).toEqual([255, 0, 0, 255]);
    expect(image.bottom).toEqual([0, 0, 255, 255]);
    expect(await page.evaluate(() => document.body.style.overflow)).toBe('');
  });
}

test('reset crop restores full rotated image and four turns preserve dimensions', async ({ page }) => {
  await chooseFile(page, false);
  await selectCrop(page, 0.2, 0.2, 0.6, 0.7);
  for (let i = 0; i < 4; i++) await page.getByRole('button', { name: 'Rotacionar 90°' }).click();
  await page.getByRole('button', { name: 'Redefinir recorte' }).click();
  await page.getByRole('button', { name: 'Confirmar', exact: true }).click();
  await expect(page.locator('#text')).toHaveValue('TEST OCR RESULT');
  expect(await page.evaluate(() => [window.ocrCalls[0].width, window.ocrCalls[0].height])).toEqual([1000, 600]);
});

for (const camera of [false, true]) {
  for (const escape of [false, true]) {
    test(`editor cancel cleans up without picker or OCR: camera=${camera}, escape=${escape}`, async ({ page }) => {
      let pickers = 0;
      page.on('filechooser', () => pickers++);
      await page.evaluate(() => { document.body.style.overflow = 'auto'; });
      if (camera) {
        await openCamera(page);
        await page.getByRole('button', { name: 'Tirar foto' }).click();
      } else await chooseFile(page, false);
      await expect(page.getByRole('button', { name: 'Confirmar', exact: true })).toBeEnabled();
      if (escape) await page.keyboard.press('Escape');
      else await page.getByRole('button', { name: 'Cancelar', exact: true }).click();
      await expect(page.locator('#image-editor-overlay')).toHaveCount(0);
      await expect(page.locator('#scan-status')).toHaveText('Edição de imagem cancelada.');
      await expect(page.locator('#btn-gallery')).toBeEnabled();
      expect(await page.evaluate(() => document.body.style.overflow)).toBe('auto');
      expect(await page.evaluate(() => window.ocrCalls.length)).toBe(0);
      expect(pickers).toBe(camera ? 0 : 1);
      if (camera) await expectCleanup(page);
      await chooseFile(page);
      await expect(page.locator('#text')).toHaveValue('TEST OCR RESULT');
    });
  }
}

test('editor restores focus and scroll after encoding failure and removes exit listeners', async ({ page }) => {
  await page.evaluate(() => {
    window.nativeToBlob = HTMLCanvasElement.prototype.toBlob;
    document.body.style.overflow = 'scroll';
    const trigger = document.createElement('button');
    trigger.id = 'editor-test-trigger';
    trigger.textContent = 'Editor test';
    trigger.onclick = async () => {
      const bridge = await import('/bridge.js');
      try { await bridge.chooseImage(true); } catch (error) { window.editorError = error.message; }
    };
    document.body.append(trigger);
  });
  const chooserPromise = page.waitForEvent('filechooser');
  await page.locator('#editor-test-trigger').click();
  const chooser = await chooserPromise;
  const png = await page.evaluate(() => {
    const canvas = document.createElement('canvas');
    canvas.width = canvas.height = 20;
    return canvas.toDataURL().split(',')[1];
  });
  await chooser.setFiles({ name: 'test.png', mimeType: 'image/png', buffer: Buffer.from(png, 'base64') });
  await expect(page.getByRole('button', { name: 'Confirmar', exact: true })).toBeEnabled();
  await page.evaluate(() => { HTMLCanvasElement.prototype.toBlob = callback => callback(null); });
  await page.getByRole('button', { name: 'Confirmar', exact: true }).click();
  await expect(page.locator('#image-editor-overlay')).toHaveCount(0);
  await expect(page.locator('#editor-test-trigger')).toBeFocused();
  expect(await page.evaluate(() => window.editorError)).toContain('Não foi possível preparar');
  expect(await page.evaluate(() => document.body.style.overflow)).toBe('scroll');
  await page.evaluate(() => {
    HTMLCanvasElement.prototype.toBlob = window.nativeToBlob;
    document.body.style.overflow = 'auto';
  });
  await page.keyboard.press('Escape');
  expect(await page.evaluate(() => document.body.style.overflow)).toBe('auto');
  expect(await page.evaluate(() => window.ocrCalls.length)).toBe(0);
});

test('invalid image decode rejects and cleans editor without reopening gallery', async ({ page }) => {
  let pickers = 0;
  page.on('filechooser', () => pickers++);
  const chooserPromise = page.waitForEvent('filechooser');
  await page.locator('#btn-gallery').click();
  await (await chooserPromise).setFiles({ name: 'broken.png', mimeType: 'image/png', buffer: Buffer.from('invalid image') });
  await expect(page.locator('#scan-status')).toContainText('Não foi possível abrir a imagem');
  await expect(page.locator('#image-editor-overlay')).toHaveCount(0);
  await expect(page.locator('#btn-gallery')).toBeEnabled();
  expect(await page.evaluate(() => document.body.style.overflow)).toBe('');
  expect(await page.evaluate(() => window.ocrCalls.length)).toBe(0);
  expect(pickers).toBe(1);
});

for (const [width, height] of [[5001, 5000], [10001, 1], [1, 10001]]) {
  test(`oversized image ${width}x${height} rejects before rendering canvases`, async ({ page }) => {
    await page.evaluate(() => {
      const drawImage = CanvasRenderingContext2D.prototype.drawImage;
      window.imageDraws = 0;
      CanvasRenderingContext2D.prototype.drawImage = function(...args) {
        window.imageDraws++;
        return drawImage.apply(this, args);
      };
    });
    const chooserPromise = page.waitForEvent('filechooser');
    await page.locator('#btn-gallery').click();
    await (await chooserPromise).setFiles({ name: 'large.svg', mimeType: 'image/svg+xml',
      buffer: Buffer.from(`<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="${height}"></svg>`) });
    await expect(page.locator('#scan-status')).toContainText('A imagem deve ter no máximo 25 milhões de pixels');
    await expect(page.locator('dialog')).toHaveCount(0);
    await expect(page.locator('#btn-gallery')).toBeEnabled();
    expect(await page.evaluate(() => window.imageDraws)).toBe(0);
    expect(await page.evaluate(() => window.ocrCalls.length)).toBe(0);
  });
}

test('OCR failure displays status and releases worker; next selection succeeds', async ({ page }) => {
  await page.evaluate(() => { window.failOcr = true; });
  await chooseFile(page);
  await expect(page.locator('#scan-status')).toContainText('Não foi possível ler a imagem');
  await expect(page.locator('#btn-gallery')).toBeEnabled();
  expect(await page.evaluate(() => window.ocrTerminated)).toBe(1);
  await page.evaluate(() => { window.failOcr = false; });
  await chooseFile(page);
  await expect(page.locator('#text')).toHaveValue('TEST OCR RESULT');
  await expect(page.locator('#scan-status')).toBeHidden();
});

test('disabled cloud OCR sends only authenticated config probe and falls back to Tesseract', async ({ page }) => {
  const requests = [];
  await page.route('**/api/capture/ocr/**', async route => {
    const request = route.request();
    requests.push({ path: new URL(request.url()).pathname, method: request.method(),
      auth: request.headers()['authorization'] || null,
      sequence: request.headers()['x-capture-sequence'] || null,
      body: request.postDataBuffer() || null });
    await route.continue();
  });
  await chooseFile(page);
  await expect(page.locator('#text')).toHaveValue('TEST OCR RESULT');
  expect(requests.map(r => r.path)).toEqual(['/api/capture/ocr/config']);
  const config = requests[0];
  expect(config.method).toBe('GET');
  expect(config.auth).toMatch(/^Bearer [0-9a-f-]{36}$/);
  expect(config.sequence).toBe('1');
  expect(await page.evaluate(() => window.ocrCalls.length)).toBe(1);
});


test('enabled cloud OCR uploads edited confirmed image without consent dialog, without Tesseract', async ({ page }) => {
  const uploads = [];
  await page.route('**/api/capture/ocr/config', route => route.fulfill({ status: 200, contentType: 'application/json', body: '{"enabled":true}' }));
  await page.route('**/api/capture/ocr/config/', route => route.fulfill({ status: 200, contentType: 'application/json', body: '{"enabled":true}' }));
  await page.route('**/api/capture/ocr', async route => {
    const request = route.request();
    uploads.push({ method: request.method(), type: request.headers()['content-type'] || '',
      size: (request.postDataBuffer() || []).length,
      auth: request.headers()['authorization'] || null,
      sequence: request.headers()['x-capture-sequence'] || null });
    await route.fulfill({ status: 200, contentType: 'application/json', body: '{"text":"CLOUD OCR RESULT"}' });
  });
  const dialogs = [];
  page.on('dialog', async dialog => { dialogs.push(dialog.type()); await dialog.dismiss(); });
  await chooseFile(page);
  await expect(page.locator('#image-editor-overlay')).toHaveCount(0);
  await expect(page.locator('#text')).toHaveValue('CLOUD OCR RESULT');
  expect(await page.evaluate(() => window.ocrCalls.length)).toBe(0);
  expect(uploads).toHaveLength(1);
  expect(uploads[0].method).toBe('POST');
  expect(uploads[0].type).toBe('image/jpeg');
  expect(uploads[0].size).toBeGreaterThan(0);
  expect(uploads[0].size).toBeLessThanOrEqual(4 * 1024 * 1024);
  expect(uploads[0].auth).toMatch(/^Bearer [0-9a-f-]{36}$/);
  expect(uploads[0].sequence).toBe('1');
  expect(dialogs).toEqual([]);
  await page.locator('#btn-discard').click();
  await chooseFile(page);
  await expect(page.locator('#text')).toHaveValue('CLOUD OCR RESULT');
  expect(await page.evaluate(() => window.ocrCalls.length)).toBe(0);
  expect(uploads).toHaveLength(2);
  expect(dialogs).toEqual([]);
});

test('cloud failure falls back to local Tesseract', async ({ page }) => {
  const dialogs = [];
  page.on('dialog', async dialog => { dialogs.push(dialog.type()); await dialog.dismiss(); });
  await page.route('**/api/capture/ocr/config', route => route.fulfill({ status: 200, contentType: 'application/json', body: '{"enabled":true}' }));
  await page.route('**/api/capture/ocr/config/', route => route.fulfill({ status: 200, contentType: 'application/json', body: '{"enabled":true}' }));
  await page.route('**/api/capture/ocr', route => route.fulfill({ status: 429, contentType: 'application/json', body: '{"error":"quota"}' }));
  await chooseFile(page);
  await expect(page.locator('#text')).toHaveValue('TEST OCR RESULT');
  expect(await page.evaluate(() => window.ocrCalls.length)).toBe(1);
  await page.unroute('**/api/capture/ocr');
  await page.route('**/api/capture/ocr', route => route.fulfill({ status: 401, contentType: 'application/json', body: '{"error":"expired"}' }));
  await page.locator('#btn-discard').click();
  await chooseFile(page);
  await expect(page.locator('#text')).toHaveValue('TEST OCR RESULT');
  expect(await page.evaluate(() => window.ocrCalls.length)).toBe(2);
  expect(dialogs).toEqual([]);
});