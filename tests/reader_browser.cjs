/* Optional browser verification, invoked by reader.py with LEAVES_BROWSER_TEST=1.
 * Requires Playwright and its Chromium browser; PLAYWRIGHT_MODULE may override
 * the module location and NODE may select the Node binary. */
const { chromium } = require(process.env.PLAYWRIGHT_MODULE || 'playwright');
const assert = require('node:assert/strict');
(async () => {
  const [base, id, author] = process.argv.slice(2);
  const browser = await chromium.launch({ headless: true });
  try {
    const page = await browser.newPage();
    const errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(`${base}/books/${id}/read`);
    await page.waitForFunction(() => document.querySelector('#chapters').options.length === 3);
    await page.frameLocator('#viewer iframe').locator('h1').filter({ hasText: 'Eine Geschichte' }).waitFor();
    assert.equal(await page.locator('#previous').isDisabled(), true);
    await page.locator('#next').click();
    await page.frameLocator('#viewer iframe').locator('h1').filter({ hasText: 'Die Fortsetzung' }).waitFor();
    await page.waitForFunction(() => document.querySelector('#next').disabled);
    await page.locator('#font-size').selectOption('150');
    await page.waitForFunction(key => JSON.parse(localStorage.getItem(key)).fontSize === '150', `leaves:reader:${id}`);
    await page.reload();
    await page.frameLocator('#viewer iframe').locator('h1').filter({ hasText: 'Die Fortsetzung' }).waitFor();
    assert.equal(await page.locator('#font-size').inputValue(), '150');
    await page.locator('#chapters').selectOption('chapter.xhtml');
    await page.frameLocator('#viewer iframe').locator('h1').filter({ hasText: 'Eine Geschichte' }).waitFor();
    const sandbox = await page.locator('#viewer iframe').getAttribute('sandbox');
    assert.ok(!sandbox.includes('allow-scripts'));
    await page.setViewportSize({ width: 390, height: 844 });
    await page.waitForTimeout(300);
    assert.ok(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth));
    await page.getByRole('link', { name: author, exact: true }).click();
    assert.equal(new URL(page.url()).searchParams.get('author'), author);
    assert.equal(await page.locator('.book').count(), 2);
    // Book loading must work when Local Storage is unavailable, too.
    await page.addInitScript(() => { Object.defineProperty(window, 'localStorage', { get() { throw new Error('blocked'); } }); });
    await page.goto(`${base}/books/${id}/read`);
    await page.frameLocator('#viewer iframe').locator('h1').filter({ hasText: 'Eine Geschichte' }).waitFor();
    assert.deepEqual(errors, []);
    console.log('OK: browser EPUB rendering, pages, chapters, font size, saved position, sandbox, mobile layout, author link, blocked storage');
  } finally { await browser.close(); }
})().catch(error => { console.error(error); process.exitCode = 1; });
