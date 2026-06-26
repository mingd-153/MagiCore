// MegaGate Core NAPI bindings - JavaScript wrapper
const path = require('path');
const binding = require('./megagate_core.node');

module.exports = {
  napiMegagateInstall: binding.napiMegagateInstall,
  napiMegagateAdd: binding.napiMegagateAdd,
  napiMegagateUpdate: binding.napiMegagateUpdate,
  napiMegagateRemove: binding.napiMegagateRemove,
  napiMegagateList: binding.napiMegagateList,
  napiMegagateLockVerify: binding.napiMegagateLockVerify,
};