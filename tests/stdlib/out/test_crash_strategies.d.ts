export interface RocaError {
  name: string;
  message: string;
}
export type RocaResult<T> = { value: T; err: null } | { value: null; err: RocaError };

export declare function crash_fallback(path: string): string;
export declare function crash_skip(path: string): string;
export declare function crash_halt(path: string): RocaResult<string>;
export declare function crash_log_fallback(path: string): string;
export declare function crash_log_halt(path: string): RocaResult<string>;
export declare function crash_detailed(path: string): RocaResult<string>;