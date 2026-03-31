export interface RocaError {
  name: string;
  message: string;
}
export type RocaResult<T> = { value: T; err: null } | { value: null; err: RocaError };

export declare function test_parse_halt(raw: string): RocaResult<string>;
export declare function test_parse_fallback(raw: string): string;