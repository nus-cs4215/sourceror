import { createContext } from "js-slang";
import { compile , Transcoder, run} from "../src/index";

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
    (returnedValue: any) => {
      return {resultStatus: 'finished', result: returnedValue, errors: null};
    })
  .catch((e: any) => {
    return {resultStatus: 'error', result: "", errors: e};
  })
}

