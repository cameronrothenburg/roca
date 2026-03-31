export interface RocaError {
  name: string;
  message: string;
}
export type RocaResult<T> = { value: T; err: null } | { value: null; err: RocaError };

export declare function both_fallback(n: number): number;
export declare function halt_then_fallback(n: number): RocaResult<number>;
export declare function both_halt(n: number): RocaResult<number>;
export declare function log_then_halt(n: number): RocaResult<number>;