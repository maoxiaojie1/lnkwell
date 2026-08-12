// 生成一个 256x256 的 PNG 图标源文件（供 `tauri icon` 使用），纯 Node 内置模块实现
const zlib = require('zlib');
const fs = require('fs');
const path = require('path');

function crc32(buf) {
  const table = [];
  for (let n = 0; n < 256; n++) {
    let c = n;
    for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
    table[n] = c >>> 0;
  }
  let c = 0xffffffff;
  for (const byte of buf) c = table[(c ^ byte) & 0xff] ^ (c >>> 8);
  return (c ^ 0xffffffff) >>> 0;
}

function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([len, body, crc]);
}

const W = 256;
const H = 256;
const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(W, 0);
ihdr.writeUInt32BE(H, 4);
ihdr[8] = 8; // bit depth
ihdr[9] = 6; // RGBA

// 蓝色圆 + 深色描边，简洁的占位图标
const raw = Buffer.alloc(H * (1 + W * 4));
for (let y = 0; y < H; y++) {
  const row = y * (1 + W * 4);
  raw[row] = 0;
  for (let x = 0; x < W; x++) {
    const o = row + 1 + x * 4;
    const dx = x - 128;
    const dy = y - 128;
    const r = Math.sqrt(dx * dx + dy * dy);
    if (r < 100) {
      raw[o] = 9; raw[o + 1] = 105; raw[o + 2] = 218; raw[o + 3] = 255;
    } else if (r < 112) {
      raw[o] = 31; raw[o + 1] = 35; raw[o + 2] = 40; raw[o + 3] = 255;
    } else {
      raw[o + 3] = 0;
    }
  }
}

const png = Buffer.concat([
  Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]),
  chunk('IHDR', ihdr),
  chunk('IDAT', zlib.deflateSync(raw, { level: 9 })),
  chunk('IEND', Buffer.alloc(0)),
]);

const out = path.join(__dirname, '..', 'app-icon.png');
fs.writeFileSync(out, png);
console.log('written:', out, png.length, 'bytes');
