const test = require('ava');
const withPage = require('./_withPage');
const compile = require('../../compileAndRunTest');

test('standalone block statements: result', withPage, async (t, page) => {
	await page.exposeFunction('runTest', compile.compileAndRunTest);
	const res = await page.evaluate(async () => {
		return await runTest(`
        function test(){
          const x = true;
          {
              const x = false;
          }
          return x;
        }
        test();
        `, 1);
	})
  t.is(res.resultStatus, "finished");
	t.is(res.result, true);
	t.is(res.errors, null);
});

test('const uses block scoping instead of function scoping', withPage, async (t, page) => {
	await page.exposeFunction('runTest', compile.compileAndRunTest);
	const res = await page.evaluate(async () => {
		return await runTest(`
        function test(){
          const x = true;
          if(true) {
              const x = false;
          } else {
              const x = false;
          }
          return x;
        }
        test();
      `, 1);
	})
  t.is(res.resultStatus, "finished");
	t.is(res.result, true);
	t.is(res.errors, null);
});

test('Error when accessing temporal dead zone', withPage, async (t, page) => {
	await page.exposeFunction('runTest', compile.compileAndRunTest);
	const res = await page.evaluate(async () => {
		return await runTest(`
        import { display } from "std/misc";
        const a = 1;
        function f() {
          display(a);
          const a = 5;
        }
        f();
        `, 1);
	})
    t.timeout(30000);
    t.is(res.resultStatus, "error");
    t.is(res.result, '');
    t.not(res.errors, null);
});

test('In a block, every going-to-be-defined variable in the block cannot be accessed until it has been defined in the block.', withPage, async (t, page) => {
	await page.exposeFunction('runTest', compile.compileAndRunTest);
	const res = await page.evaluate(async () => {
		return await runTest(`
        const a = 1;
        {
          a + a;
          const a = 10;
        }
        `, 1);
	})
    t.timeout(30000);
    t.is(res.resultStatus, "error");
    t.not(res.errors, null);
    t.is(res.result, '');
});

