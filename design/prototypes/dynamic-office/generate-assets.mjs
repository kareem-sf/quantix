import { Avatar, Style } from '@dicebear/core';
import definition from '@dicebear/styles/notionists.json' with { type: 'json' };
import { mkdir, writeFile, copyFile } from 'node:fs/promises';

await mkdir(new URL('./assets/', import.meta.url), { recursive: true });
const style = new Style(definition);
for (const [id, seed] of Object.entries({ manager: 'quantix-manager-24', lina: 'LinaHaddad-29', samir: 'SamirFares-42', nour: 'NourSelim-18' })) {
  const avatar = new Avatar(style, { seed, size: 160 });
  await writeFile(new URL(`./assets/${id}.svg`, import.meta.url), avatar.toString());
}
for (const weight of [400, 500, 600]) {
  await copyFile(new URL(`../../../node_modules/@fontsource/inter/files/inter-latin-${weight}-normal.woff2`, import.meta.url), new URL(`./assets/inter-${weight}.woff2`, import.meta.url));
}
await copyFile(new URL('../../../node_modules/@fontsource/inter/LICENSE', import.meta.url), new URL('./assets/INTER-LICENSE.txt', import.meta.url));
await writeFile(new URL('./assets/ATTRIBUTION.txt', import.meta.url), 'Illustrations: DiceBear Notionists, derived from Notionists by Zoish, CC0 1.0.\nhttps://www.dicebear.com/styles/notionists/\nLocally rendered demonstration identities only; no production employee roster.\nInter: bundled existing @fontsource/inter assets; see INTER-LICENSE.txt.\n');
console.log('Local illustrated portraits and Inter fonts ready.');
