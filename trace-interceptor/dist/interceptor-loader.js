"use strict";
/**
 * Interceptor loader - entry point for NODE_OPTIONS --require
 *
 * Usage: NODE_OPTIONS="--require /path/to/dist/interceptor-loader.js" claude
 */
Object.defineProperty(exports, "__esModule", { value: true });
const index_1 = require("./index");
// Auto-install on require
(0, index_1.install)();
