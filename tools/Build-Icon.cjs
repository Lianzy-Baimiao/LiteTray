// Render the editable SVG into a Windows icon containing exact DPI sizes.
// Usage: node tools/Build-Icon.cjs (requires sharp; NODE_PATH is supported).
const fs = require('node:fs');
const path = require('node:path');
const sharp = require('sharp');
const root = path.resolve(__dirname, '..');
(async () => {
  const sizes = [16, 20, 24, 32, 40, 48, 64, 128, 256];
  const svg = fs.readFileSync(path.join(root, 'assets/litetray.svg'));
  const frames = await Promise.all(sizes.map(size => sharp(svg, { density: 384 }).resize(size, size).png().toBuffer()));
  const header = Buffer.alloc(6 + 16 * sizes.length);
  header.writeUInt16LE(1, 2);
  header.writeUInt16LE(sizes.length, 4);
  let offset = header.length;
  frames.forEach((frame, i) => {
    const entry = 6 + i * 16;
    header[entry] = sizes[i] % 256;
    header[entry + 1] = sizes[i] % 256;
    header.writeUInt16LE(1, entry + 4);
    header.writeUInt16LE(32, entry + 6);
    header.writeUInt32LE(frame.length, entry + 8);
    header.writeUInt32LE(offset, entry + 12);
    offset += frame.length;
  });
  fs.writeFileSync(path.join(root, 'assets/litetray.ico'), Buffer.concat([header, ...frames]));
  fs.mkdirSync(path.join(root, 'local'), { recursive: true });
  fs.writeFileSync(path.join(root, 'local/litetray-icon-preview.png'), frames.at(-1));
  console.log(`Built LiteTray icon: ${sizes.join(', ')} px`);
})().catch(error => { console.error(error); process.exitCode = 1; });
