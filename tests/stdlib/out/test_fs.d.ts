export interface RocaError {
  name: string;
  message: string;
}
export type RocaResult<T> = { value: T; err: null } | { value: null; err: RocaError };

export declare function test_read_with_fallback(path: string): string;
export declare function test_read_with_skip(path: string): string;
export declare function test_read_halt(path: string): RocaResult<string>;
export declare function test_read_detailed(path: string): string;
export declare function test_write_halt(path: string, content: string): RocaResult<void>;
export declare function test_readdir_halt(path: string): RocaResult<number>;