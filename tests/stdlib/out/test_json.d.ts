export interface RocaError {
  name: string;
  message: string;
}
export type RocaResult<T> = { value: T; err: null } | { value: null; err: RocaError };

export declare function test_parse_valid(text: string): string;
export declare function test_parse_with_fallback(text: string): string;
export declare function test_parse_with_skip(text: string): string;
export declare function test_parse_with_log(text: string): string;
export declare function test_parse_error(text: string): RocaResult<string>;