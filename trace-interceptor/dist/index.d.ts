/**
 * Deciduous Trace Interceptor
 *
 * Intercepts fetch() calls to the Anthropic API and records them to deciduous.
 *
 * Usage: NODE_OPTIONS="--require /path/to/dist/interceptor-loader.js" claude
 */
import { DeciduousClient } from './deciduous';
import { ResponseAccumulator } from './stream-parser';
export declare function debugLog(msg: string): void;
/**
 * Install the interceptor
 */
export declare function install(): void;
export { DeciduousClient, ResponseAccumulator };
