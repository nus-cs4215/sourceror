const test = require('ava');
const withPage = require('./_withPage');
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

test('Tail call in boolean operators work', withPage, async (t, page) => {
	await page.exposeFunction('runTest', compile.compileAndRunTest);
	const res = await page.evaluate(async () => {
		return await runTest(`
    function f(x, y) {
      if (x <= 0) {
        return y;
      } else {
        return false || f(x-1, y+1);
      }
    }
    f(5000, 5000);
    `, 1);
	})
	t.is(res.resultStatus, "finished");
	t.is(res.result, 10000);
	t.is(res.errors, null);
});

test('Tail call in nested mix of conditional expressions boolean operators work', withPage, async (t, page) => {
	await page.exposeFunction('runTest', compile.compileAndRunTest);
	const res = await page.evaluate(async () => {
		return await runTest(`
    function f(x, y) {
      return x <= 0 ? y : false || x > 0 ? f(x-1, y+1) : 'unreachable';
    }
    f(5000, 5000);
    `, 1);
	})
	t.is(res.resultStatus, "finished");
	t.is(res.result, 10000);
	t.is(res.errors, null);
});

test('Tail calls in arrow functions work', withPage, async (t, page) => {
	await page.exposeFunction('runTest', compile.compileAndRunTest);
	const res = await page.evaluate(async () => {
		return await runTest(`
    const f = (x, y) => x <= 0 ? y : f(x-1, y+1);
    f(5000, 5000);
    `, 1);
	})
	t.is(res.resultStatus, "finished");
	t.is(res.result, 10000);
	t.is(res.errors, null);
});

test('Tail calls in arrow block functions work', withPage, async (t, page) => {
	await page.exposeFunction('runTest', compile.compileAndRunTest);
	const res = await page.evaluate(async () => {
		return await runTest(`
    const f = (x, y) => {
      if (x <= 0) {
        return y;
      } else {
        return f(x-1, y+1);
      }
    };
    f(5000, 5000);
    `, 1);
	})
	t.is(res.resultStatus, "finished");
	t.is(res.result, 10000);
	t.is(res.errors, null);
});

test('Tail calls in block functions work', withPage, async (t, page) => {
	await page.exposeFunction('runTest', compile.compileAndRunTest);
	const res = await page.evaluate(async () => {
		return await runTest(`
    function f(x, y) {
      if (x <= 0) {
        return y;
      } else {
        return g(x-1, y+1);
      }
    }
    function g(x, y) {
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

test('Tail calls in mutual recursion with arrow functions work', withPage, async (t, page) => {
	await page.exposeFunction('runTest', compile.compileAndRunTest);
	const res = await page.evaluate(async () => {
		return await runTest(`
    const f = (x, y) => x <= 0 ? y : g(x-1, y+1);
    const g = (x, y) => x <= 0 ? y : f(x-1, y+1);
    f(5000, 5000);
    `, 1);
	})
	t.is(res.resultStatus, "finished");
	t.is(res.result, 10000);
	t.is(res.errors, null);
});

test('Tail calls in mixed tail-call/non-tail-call recursion work', withPage, async (t, page) => {
	await page.exposeFunction('runTest', compile.compileAndRunTest);
	const res = await page.evaluate(async () => {
		return await runTest(`
    function f(x, y, z) {
      if (x <= 0) {
        return y;
      } else {
        return f(x-1, y+f(0, z, 0), z);
      }
    }
    f(5000, 5000, 2);
    `, 1);
	})
	t.is(res.resultStatus, "finished");
  	t.is(res.result, 15000);
	t.is(res.errors, null);
});

// Make test valid after merging TCO
test.skip('Tail Call Optimization works', withPage, async (t, page) => {
	await page.exposeFunction('runTest', compile.compileAndRunTest);
	const res = await page.evaluate(async () => {
		return await runTest(`
    function arithmetic_sum(number, total) {
        return number === 0 ? total : arithmetic_seq(number - 1, total + number);
    }
    arithmetic_seq(20000, 0);
    `, 1);
	})
	t.timeout(10000)
	t.is(res.resultStatus, "finished");
	t.is(res.result, 200010000);
	t.is(res.errors, null);
});

test.todo('Test tail calls with WASM tail call proposal by enabling flags');