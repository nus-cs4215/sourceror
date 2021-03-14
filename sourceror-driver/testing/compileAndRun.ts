import {
  createContext
} from 'js-slang';
import { compile, run, Transcoder, makePlatformImports } from "../src/index";

export function compileAndRunTest(code: string, chapter = 1): Promise<any> { // TestResult?
  let context = createContext(chapter);
  return compile(code, context)
  .then((wasmModule: WebAssembly.Module) => {
    const transcoder = new Transcoder();
    return run(
      wasmModule,
      makePlatformImports({}, transcoder),
      transcoder,
      context
    );
  })
.then(
  (returnedValue: any): any => {
    return { resultStatus: 'finished', result: returnedValue, errors: []};
  },
  (e: any): any => {
    return { resultStatus: 'error', errors: e};
  }
).catch((err) => {
  console.log('error: ', err);
})
}

compileAndRunTest("1;").then(res => console.log(res));
