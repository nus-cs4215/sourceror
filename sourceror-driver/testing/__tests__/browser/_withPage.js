const puppeteer = require('puppeteer');
const compile = require('../../compileAndRunTest');

module.exports = async (t, run) => {
	const browser = await puppeteer.launch({
		headless: true
	  });
	const page = await browser.newPage();
	await page.exposeFunction('compileAndRunTest', compile.compileAndRunTest);
	try {
		await run(t, page);
	} finally {
		await page.close();
		await browser.close();
	}
}