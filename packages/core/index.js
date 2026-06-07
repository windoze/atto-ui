'use strict'

const native = require('./native')

exports.AppHost = native.AppHost
exports.registerAllRuntimeComponents = native.registerAllRuntimeComponents
exports.version = native.version

Object.assign(exports, require('./src/builders'))
