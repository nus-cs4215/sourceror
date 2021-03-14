import { Context, Variant, SourceError, Value } from '../types'
import { compileAndRunTest }  from '../compileAndRun'

export interface TestContext extends Context {
  displayResult: string[]
  promptResult: string[]
  alertResult: string[]
  visualiseListResult: Value[]
}

interface TestBuiltins {
  [builtinName: string]: any
}

export interface TestResult {
  code: string
  displayResult: string[]
  alertResult: string[]
  visualiseListResult: any[]
  errors: SourceError[]
  parsedErrors: string
  resultStatus: string
  result: Value
}

interface TestOptions {
  context?: TestContext
  chapter?: number
  variant?: Variant
  testBuiltins?: TestBuiltins
  native?: boolean
}

export function expectResult(code: string, options: TestOptions = {}) {
  return expect(
    testSuccess(code, options)
      .then(testResult => testResult.result)
  ).resolves
}

export async function testSuccess(code: string, options: TestOptions = { native: false }) {
  const testResult = await compileAndRunTest(code)
  expect(testResult.errors).toEqual([])
  expect(testResult.resultStatus).toBe('finished')
  return testResult
}

export async function testSuccessWithErrors(
  code: string,
  options: TestOptions = { native: false }
) {
  const testResult = await compileAndRunTest(code)
  expect(testResult.errors).not.toEqual([])
  expect(testResult.resultStatus).toBe('finished')
  return testResult
}

export async function testFailure(code: string, options: TestOptions = { native: false }) {
  const testResult = await compileAndRunTest(code)
  expect(testResult.errors).not.toEqual([])
  expect(testResult.resultStatus).toBe('error')
  return testResult
}

