const native = require('..');

if (native.version() !== '0.1.0') {
  throw new Error(`Expected version 0.1.0, got ${native.version()}`);
}
