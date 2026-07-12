// Preload stays minimal: no Node APIs exposed to the page.
// The SPA talks only to the local loopback HTTP API.

'use strict';

const { contextBridge } = require('electron');

contextBridge.exposeInMainWorld('statsCodeDesktop', {
  shell: 'electron',
  version: '0.1.0',
});
