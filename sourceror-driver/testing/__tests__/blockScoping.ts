import { stripIndent } from '../utils/formatters'
import { expectResult } from '../utils/testing'
import { Constants } from '../../src/constants/constants'
import {jest, expect, test} from '@jest/globals'

jest.useFakeTimers();

// This is bad practice. Don't do this!
test('standalone block statements', () => {
    return expectResult(
      stripIndent`
      function test(){
        const x = true;
        {
            const x = false;
        }
        return x;
      }
      test();
    `,
      { chapter: 1 }
    ).toMatchInlineSnapshot(`true`)
  })

// This is bad practice. Don't do this!
test('const uses block scoping instead of function scoping', () => {
    return expectResult(
      stripIndent`
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
    `,
    { chapter: 1 }
    ).toMatchInlineSnapshot(`true`)
})

test('Error when accessing temporal dead zone', () => {
    return expectResult(stripIndent`
      import { display } from "std/misc";
      const a = 1;
      function f() {
        display(a);
        const a = 5;
      }
      f();
      `, 
      { chapter: 1 }
      ).toEqual(expect.stringMatching(Constants.RuntimeErrors.VARIABLE_INIT))
})

test('In a block, every going-to-be-defined variable in the block cannot be accessed until it has been defined in the block.', () => {
    return expectResult(stripIndent`
        const a = 1;
        {
          a + a;
          const a = 10;
        }
      `, 
      { chapter: 1 }
      ).toEqual(expect.stringMatching(Constants.RuntimeErrors.VARIABLE_INIT))
})