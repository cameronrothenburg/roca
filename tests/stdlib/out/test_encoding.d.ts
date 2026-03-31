export interface RocaError {
  name: string;
  message: string;
}
export type RocaResult<T> = { value: T; err: null } | { value: null; err: RocaError };

export declare function test_btoa(input: string): string;
export declare function test_atob(input: string): RocaResult<string>;
export declare function test_atob_fallback(input: string): string;