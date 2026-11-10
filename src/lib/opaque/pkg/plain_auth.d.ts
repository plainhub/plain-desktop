/* tslint:disable */
/* eslint-disable */

export class OpaqueLogin {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    finish(response: Uint8Array, cid: string, server_signing_key: Uint8Array): OpaqueLoginResult;
    request(): Uint8Array;
    static start(password: Uint8Array): OpaqueLogin;
}

export class OpaqueLoginResult {
    private constructor();
    free(): void;
    [Symbol.dispose](): void;
    readonly apiKey: Uint8Array;
    readonly ke3: Uint8Array;
    readonly requestAad: Uint8Array;
    readonly responseAad: Uint8Array;
    readonly serverPublicKey: Uint8Array;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_opaquelogin_free: (a: number, b: number) => void;
    readonly __wbg_opaqueloginresult_free: (a: number, b: number) => void;
    readonly opaquelogin_finish: (a: number, b: number, c: number, d: number, e: number, f: number, g: number, h: number) => void;
    readonly opaquelogin_request: (a: number, b: number) => void;
    readonly opaquelogin_start: (a: number, b: number, c: number) => void;
    readonly opaqueloginresult_apiKey: (a: number, b: number) => void;
    readonly opaqueloginresult_ke3: (a: number, b: number) => void;
    readonly opaqueloginresult_requestAad: (a: number, b: number) => void;
    readonly opaqueloginresult_responseAad: (a: number, b: number) => void;
    readonly opaqueloginresult_serverPublicKey: (a: number, b: number) => void;
    readonly __wbindgen_export: (a: number) => void;
    readonly __wbindgen_add_to_stack_pointer: (a: number) => number;
    readonly __wbindgen_export2: (a: number, b: number) => number;
    readonly __wbindgen_export3: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_export4: (a: number, b: number, c: number) => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
