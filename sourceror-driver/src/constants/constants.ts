export namespace Constants
{
    export abstract class RuntimeErrors {
        static readonly GENERAL: string = "General runtime error";
        static readonly MEMORY: string = "Out of memory";
        static readonly TYPE: string = "General runtime type error";
        static readonly PARAMETER_TYPE: string = "Function called with incorrect parameter type";
        static readonly UNARY_PARAMETER_TYPE: string = "Unary operator called with incorrect parameter type";
        static readonly BINARY_PARAMETER_TYPE: string = "Binary operator called with incorrect parameter type";
        static readonly NON_FUNCTION: string = "Function call operator applied on a non-function";
        static readonly NON_BOOLEAN: string = "If statement has a non-boolean condition";
        static readonly VARIABLE_INIT: string = "Variable used before initialization";
        static readonly UNKNOWN: string = "Unknown runtime error";
    }
}
