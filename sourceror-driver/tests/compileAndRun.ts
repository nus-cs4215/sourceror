import {
  createContext
} from 'js-slang';
import { compile, run, Transcoder, makePlatformImports } from "../src/index";
import { TestResult } from "../tests/utils/testing";

export function compileAndRunTest(code: string, chapter = 1): Promise<any> { // TestResult?
  let context= createContext(chapter);
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
    return { status: 'finished', value: returnedValue, errors: []};
  },
  (e: any): any => {
    return { status: 'error', errors: e};
  }
);
}
