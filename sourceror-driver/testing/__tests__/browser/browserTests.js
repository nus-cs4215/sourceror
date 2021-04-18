const test = require('ava');
const withPage = require('./_withPage');
const formatter = require('../../utils/formatters');
const compile = require('../../compileAndRunTest');

test('Check that stack is at most 10k in size', withPage, async (t, page) => {
	await page.exposeFunction('runTest', compile.compileAndRunTest);
	const res = await page.evaluate(async () => {
		return await runTest(`
		function f(x) {
		  if (x <= 0) {
			return 0;
		  } else {
			return 1 + f(x-1);
		  }
		}
		f(10000);
		`, 1);
	})
	t.is(res.resultStatus, "error");
    t.is(res.result, '');
    // t.regex(res.errors.message, RegExp("^Maximum call stack size exceeded"));
});

test('Simple tail call returns work', withPage, async (t, page) => {
	await page.exposeFunction('runTest', compile.compileAndRunTest);
	const res = await page.evaluate(async () => {
		return await runTest(`
		function f(x, y) {
		  if (x <= 0) {
			return y;
		  } else {
			return f(x-1, y+1);
		  }
		}
		f(5000, 5000);
		`, 1);
	})
	t.is(res.resultStatus, "finished");
	t.is(res.result, 10000);
	t.is(res.errors, null);
});

test('Tail call in conditional expressions work', withPage, async (t, page) => {
	await page.exposeFunction('runTest', compile.compileAndRunTest);
	const res = await page.evaluate(async () => {
		return await runTest(`
		function f(x, y) {
		  return x <= 0 ? y : f(x-1, y+1);
		}
		f(5000, 5000);
		`, 1);
	})
	t.is(res.resultStatus, "finished");
	t.is(res.result, 10000);
	t.is(res.errors, null);
});