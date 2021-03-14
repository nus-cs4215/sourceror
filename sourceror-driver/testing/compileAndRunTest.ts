import {
  createContext
} from 'js-slang';
import { compile, run, Transcoder } from "../src/index";

export function compileAndRunTest(code: string, chapter = 1) { 
    let context = createContext(chapter);
    return compile(code, context)
    .then((wasmModule: WebAssembly.Module) => {
      const transcoder = new Transcoder();
      return run(
        wasmModule,
        {},
        transcoder,
        context
      );
    })
  .then(
    (returnedValue) => {
      return {resultStatus: 'finished', result: returnedValue, errors: []};
    })
  .catch((e) => {
    return {resultStatus: 'error', errors: e};
  })
}


