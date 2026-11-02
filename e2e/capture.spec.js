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
        window.ocrCalls.push({ type: file.type, size: file.size, width: bitmap.width, height: bitmap.height });
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
async function chooseFile(page) {
  const chooserPromise = page.waitForEvent('filechooser');
  await page.locator('#btn-gallery').click();
  const chooser = await chooserPromise;
  expect(await chooser.element().getAttribute('capture')).toBeNull();
  const png = await page.evaluate(() => {
    const canvas = document.createElement('canvas');
    canvas.width = 100;
    canvas.height = 60;
    canvas.getContext('2d').fillRect(0, 0, 100, 60);
    return canvas.toDataURL('image/png').split(',')[1];
  });
  await chooser.setFiles({ name: 'card.png', mimeType: 'image/png', buffer: Buffer.from(png, 'base64') });
}

test('camera previews, takes a real frame, releases tracks and submits to real session', async ({ page, request }) => {
  await expect(page.locator('video')).toHaveCount(0);
  await openCamera(page);
  expect(await page.evaluate(() => window.ocrCalls.length)).toBe(0);
  await page.getByRole('button', { name: 'Tirar foto' }).click();
  await expect(page.locator('#text')).toHaveValue('TEST OCR RESULT');
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

test('camera cancel releases tracks, does not invoke OCR or open gallery, and can retry', async ({ page }) => {
  let pickers = 0;
  page.on('filechooser', () => pickers++);
  await openCamera(page);
  await page.getByRole('button', { name: 'Cancelar', exact: true }).click();
  await expectCleanup(page);
  await expect(page.locator('#scan-status')).toHaveText('Captura cancelada.');
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
