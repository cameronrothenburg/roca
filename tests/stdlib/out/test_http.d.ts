export interface RocaError {
  name: string;
  message: string;
}
export type RocaResult<T> = { value: T; err: null } | { value: null; err: RocaError };

export declare function test_get_with_fallback(url: string): string;
export declare function test_get_halt(url: string): RocaResult<string>;
export declare function test_post_with_fallback(url: string, body: string): string;
export declare function test_get_detailed(url: string): RocaResult<string>;
export declare function test_get_retry(url: string): RocaResult<string>;
export declare function test_status_check(url: string): number;
export declare function test_ok_check(url: string): boolean;