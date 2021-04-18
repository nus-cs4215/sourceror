import test from 'ava';
import { compileAndRunTest } from "../compileAndRunTest"
import { stripIndent } from "../utils/formatters"

// This is bad practice. Don't do this!
test('standalone block statements: result', async t => {
	const code: string = stripIndent`
    function test(){
      const x = true;
      {
          const x = false;
      }
      return x;
    }
    test();
    `;
    const chapter: number = 1;
	const res = (await compileAndRunTest(code, chapter));
  t.is(res.resultStatus, "finished");
	t.is(res.result, true);
	t.is(res.errors, null);
});

// This is bad practice. Don't do this!
test('const uses block scoping instead of function scoping', async t => {
	const code: string = stripIndent`
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
  `;
    const chapter: number = 1;
	const res = (await compileAndRunTest(code, chapter));
  t.is(res.resultStatus, "finished");
	t.is(res.result, true);
	t.is(res.errors, null);
});

test('Error when accessing temporal dead zone', async t => {
	const code: string = stripIndent`
    import { display } from "std/misc";
    const a = 1;
    function f() {
      display(a);
      const a = 5;
    }
    f();
    `;
    const chapter: number = 1;
	const res = (await compileAndRunTest(code, chapter));
  t.timeout(30000);
  t.is(res.resultStatus, "error");
  t.is(res.result, '');
	t.not(res.errors, null);
});

test('In a block, every going-to-be-defined variable in the block cannot be accessed until it has been defined in the block.', async t => {
	const code: string = stripIndent`
    const a = 1;
    {
      a + a;
      const a = 10;
    }
    `;
    const chapter: number = 1;
	const res = (await compileAndRunTest(code, chapter));
  t.timeout(30000);
  t.is(res.resultStatus, "error");
	t.not(res.errors, null);
  t.is(res.result, '');
});


test.skip('let uses block scoping instead of function scoping', async t => {
	const code: string = stripIndent`
    function test(){
      let x = true;
      if(true) {
          let x = false;
      } else {
          let x = false;
      }
      return x;
    }
    test();
  `;
    const chapter: number = 3;
	const res = (await compileAndRunTest(code, chapter));
	t.is(res.result, true);
	t.is(res.errors, null);
	t.is(res.resultStatus, "finished");
});

// This is bad practice. Don't do this!
test.skip('for loops use block scoping instead of function scoping', async t => {
	const code: string = stripIndent`
    function test(){
      let x = true;
      for (let x = 1; x > 0; x = x - 1) {
      }
      return x;
    }
    test();
  `;
    const chapter: number = 3;
	const res = (await compileAndRunTest(code, chapter));
	t.is(res.result, true);
	t.is(res.errors, null);
	t.is(res.resultStatus, "finished");
});

// This is bad practice. Don't do this!
test.skip('while loops use block scoping instead of function scoping', async t => {
	const code: string = stripIndent`
    function test(){
      let x = true;
      while (true) {
        let x = false;
        break;
      }
      return x;
    }
    test();
  `;
    const chapter: number = 4;
	const res = (await compileAndRunTest(code, chapter));
	t.is(res.result, true);
	t.is(res.errors, null);
	t.is(res.resultStatus, "finished");
});

// see https://www.ecma-international.org/ecma-262/6.0/#sec-for-statement-runtime-semantics-labelledevaluation
// and https://hacks.mozilla.org/2015/07/es6-in-depth-let-and-const/
test.skip('for loop `let` variables are copied into the block scope', async t => {
	const code: string = stripIndent`
  function test(){
    let z = [];
    for (let x = 0; x < 10; x = x + 1) {
      z[x] = () => x;
    }
    return z[1]();
  }
  test();
  `;
  const chapter: number = 4;
	const res = (await compileAndRunTest(code, chapter));
	t.is(res.result, 1);
	t.is(res.errors, null);
	t.is(res.resultStatus, "finished");
});

test.skip('Cannot overwrite loop variables within a block', async t => {
	const code: string = stripIndent`
  function test(){
      let z = [];
      for (let x = 0; x < 2; x = x + 1) {
        x = 1;
      }
      return false;
  }
  test();
  `;
  const chapter: number = 3;
	const res = (await compileAndRunTest(code, chapter));
	t.not(res.errors, null);
	t.is(res.resultStatus, "error");
});

test.skip('No hoisting of functions. Only the name is hoisted like let and const', async t => {
	const code: string = stripIndent`
  const v = f();
  function f() {
    return 1;
  }
  v;
  `;
	const res = (await compileAndRunTest(code));
  t.timeout(30000);
	t.not(res.errors, null);
	t.is(res.resultStatus, "error");
});

test.skip('Shadowed variables may not be assigned to until declared in the current scope', async t => {
	const code: string = stripIndent`
  let variable = 1;
  function test(){
    variable = 100;
    let variable = true;
    return variable;
  }
  test();
  `;
  const chapter: number = 3;
	const res = (await compileAndRunTest(code, chapter));
	t.not(res.errors, null);
	t.is(res.resultStatus, "error");
});