import test from 'ava';
import { compileAndRunTest } from "../compileAndRunTest"
import { stripIndent } from "../utils/formatters"

// TODO: remove after TCO 
test('Check that stack is at most 10k in size', async t => {
	const code: string = stripIndent`
    function f(x) {
      if (x <= 0) {
        return 0;
      } else {
        return 1 + f(x-1);
      }
    }
    f(10000);
    `;
    const chapter: number = 1;
	const res = (await compileAndRunTest(code, chapter));
    t.timeout(10000);
    t.is(res.resultStatus, "error");
    t.is(res.result, '');
    t.regex(res.errors.message, RegExp("^Maximum call stack size exceeded"));
});

test('Simple tail call returns work', async t => {
	const code: string = stripIndent`
    function f(x, y) {
      if (x <= 0) {
        return y;
      } else {
        return f(x-1, y+1);
      }
    }
    f(5000, 5000);
    `;
    const chapter: number = 1;
	const res = (await compileAndRunTest(code, chapter));
  t.is(res.resultStatus, "finished");
	t.is(res.result, 10000);
	t.is(res.errors, null);
});

test('Tail call in conditional expressions work', async t => {
	const code: string = stripIndent`
    function f(x, y) {
      return x <= 0 ? y : f(x-1, y+1);
    }
    f(5000, 5000);
    `;
    const chapter: number = 1;
	const res = (await compileAndRunTest(code, chapter));
    t.is(res.resultStatus, "finished");
	t.is(res.result, 10000);
	t.is(res.errors, null);
});

test('Tail call in boolean operators work', async t => {
	const code: string = stripIndent`
    function f(x, y) {
      if (x <= 0) {
        return y;
      } else {
        return false || f(x-1, y+1);
      }
    }
    f(5000, 5000);
    `;
    const chapter: number = 1;
	const res = (await compileAndRunTest(code, chapter));
    t.is(res.resultStatus, "finished");
	t.is(res.result, 10000);
	t.is(res.errors, null);
});

test('Tail call in nested mix of conditional expressions boolean operators work', async t => {
	const code: string = stripIndent`
    function f(x, y) {
      return x <= 0 ? y : false || x > 0 ? f(x-1, y+1) : 'unreachable';
    }
    f(5000, 5000);
    `;
    const chapter: number = 1;
	const res = (await compileAndRunTest(code, chapter));
    t.is(res.resultStatus, "finished");
	t.is(res.result, 10000);
	t.is(res.errors, null);
});

test('Tail calls in arrow functions work', async t => {
	const code: string = stripIndent`
    const f = (x, y) => x <= 0 ? y : f(x-1, y+1);
    f(5000, 5000);
    `;
    const chapter: number = 1;
	const res = (await compileAndRunTest(code, chapter));
    t.is(res.resultStatus, "finished");
	t.is(res.result, 10000);
	t.is(res.errors, null);
});


test('Tail calls in arrow block functions work', async t => {
	const code: string = stripIndent`
    const f = (x, y) => {
      if (x <= 0) {
        return y;
      } else {
        return f(x-1, y+1);
      }
    };
    f(5000, 5000);
    `;
    const chapter: number = 1;
	const res = (await compileAndRunTest(code, chapter));
    t.is(res.resultStatus, "finished");
	t.is(res.result, 10000);
	t.is(res.errors, null);
});

test('Tail calls in block functions work', async t => {
	const code: string = stripIndent`
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
    `;
    const chapter: number = 1;
	const res = (await compileAndRunTest(code, chapter));
    t.is(res.resultStatus, "finished");
	t.is(res.result, 10000);
	t.is(res.errors, null);
});

test('Tail calls in mutual recursion with arrow functions work', async t => {
	const code: string =  stripIndent`
    const f = (x, y) => x <= 0 ? y : g(x-1, y+1);
    const g = (x, y) => x <= 0 ? y : f(x-1, y+1);
    f(5000, 5000);
    `;
    const chapter: number = 1;
	const res = (await compileAndRunTest(code, chapter));
    t.is(res.resultStatus, "finished");
	t.is(res.result, 10000);
	t.is(res.errors, null);
});

test('Tail calls in mixed tail-call/non-tail-call recursion work', async t => {
	const code: string =  stripIndent`
    function f(x, y, z) {
      if (x <= 0) {
        return y;
      } else {
        return f(x-1, y+f(0, z, 0), z);
      }
    }
    f(5000, 5000, 2);
    `;
    const chapter: number = 1;
	const res = (await compileAndRunTest(code, chapter));
    t.is(res.resultStatus, "finished");
	t.is(res.result, 15000);
	t.is(res.errors, null);
});

// Make test valid after merging TCO
test.skip('Tail Call Optimization works', async t => {
	const code: string =  stripIndent`
    function arithmetic_sum(number, total) {
        return number === 0 ? total : arithmetic_seq(number - 1, total + number);
    }
    arithmetic_seq(20000, 0);
    `;
    const chapter: number = 1;
	const res = (await compileAndRunTest(code, chapter));
    t.timeout(10000)
    t.is(res.resultStatus, "finished");
	t.is(res.result, 200010000);
	t.is(res.errors, null);
});

test.todo('Test tail calls with WASM tail call proposal by enabling flags');


