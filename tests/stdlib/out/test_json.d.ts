export interface RocaError {
  name: string;
  message: string;
}
export type RocaResult<T> = { value: T; err: null } | { value: null; err: RocaError };

export declare function test_parse_roundtrip(text: string): string;
export declare function test_parse_skip(text: string): string;
export declare function test_parse_halt(text: string): RocaResult<string>;
export declare function test_parse_log_halt(text: string): RocaResult<string>;