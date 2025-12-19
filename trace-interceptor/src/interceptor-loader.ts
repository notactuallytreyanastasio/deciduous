/**
 * Interceptor loader - entry point for NODE_OPTIONS --require
 *
 * Usage: NODE_OPTIONS="--require /path/to/dist/interceptor-loader.js" claude
 */

import { install } from './index';

// Auto-install on require
install();
