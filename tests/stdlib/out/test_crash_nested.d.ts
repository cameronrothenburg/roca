export interface RocaError {
  name: string;
  message: string;
}
export type RocaResult<T> = { value: T; err: null } | { value: null; err: RocaError };

export declare function level_c(path: string): RocaResult<string>;
export declare function level_b(path: string): string;
export declare function level_a(path: string): string;
export declare function strict_c(path: string): RocaResult<string>;
export declare function strict_b(path: string): RocaResult<string>;
export declare function strict_a(path: string): RocaResult<string>;