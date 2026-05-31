import { createHash } from 'crypto';
import { createReadStream, statSync } from 'fs';
import { dirname, resolve } from 'path';
import { fileURLToPath } from 'url';

interface ExpectedResource {
  rel: string;
  size: number;
  sha256: string;
}

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const resourcesDir = resolve(root, 'apps/main/src-tauri/resources');

const expected: ExpectedResource[] = [
  {
    rel: 'install-interception.exe',
    size: 470_528,
    sha256: 'E137863A79DA797F08E7A137280FF2A123809044A888FD75CE9C973198915ABE',
  },
  {
    rel: 'ddhid.63340.dll',
    size: 2_242_088,
    sha256: '01E8DB6893CF79E9E7AA3AFBEE76BEA6C4220C4D1A2C63BC2E5B7C109FDB831E',
  },
  {
    rel: 'ddhid-driver/ddc.exe',
    size: 97_240,
    sha256: '3C535B334F0897B8A0870BCB476C30EA79AFD09CFC18F8E00190BDC7C6C46785',
  },
  {
    rel: 'ddhid-driver/ddhid63340.inf',
    size: 1_685,
    sha256: '17FE3814F57E98DD2AF97F56B63502E474EA5E41CDA1A510FFE435EE6AD7A104',
  },
  {
    rel: 'ddhid-driver/ddhid63340.cat',
    size: 12_110,
    sha256: '6135C664711127A62E0988F6844521E345D78ACE9D3747A392400CE99BE96983',
  },
  {
    rel: 'ddhid-driver/ddhid63340.sys',
    size: 1_190_080,
    sha256: 'FBE510402B3822C63E94752051B7D5895B67875F22EC48593DE19764A649F8B1',
  },
  {
    rel: 'disable-ddhid-driver.cmd',
    size: 3_926,
    sha256: 'ED87578865134EB6E7D8B2A07EFDE505E7E29789490E173F647E0B2C485E9E25',
  },
];

async function sha256(path: string): Promise<string> {
  const hash = createHash('sha256');
  await new Promise<void>((resolvePromise, reject) => {
    createReadStream(path)
      .on('data', (chunk) => hash.update(chunk))
      .on('error', reject)
      .on('end', resolvePromise);
  });
  return hash.digest('hex').toUpperCase();
}

async function main() {
  let failed = false;

  for (const item of expected) {
    const path = resolve(resourcesDir, item.rel);
    try {
      const stat = statSync(path);
      if (stat.size !== item.size) {
        failed = true;
        console.error(
          `[资源异常] ${item.rel} 大小不匹配：实际 ${stat.size}，期望 ${item.size}`,
        );
        continue;
      }
      const actualHash = await sha256(path);
      if (actualHash !== item.sha256) {
        failed = true;
        console.error(`[资源异常] ${item.rel} SHA256 不匹配：`);
        console.error(`  实际 ${actualHash}`);
        console.error(`  期望 ${item.sha256}`);
        continue;
      }
      console.log(`[OK] ${item.rel}`);
    } catch (e) {
      failed = true;
      console.error(`[资源异常] ${item.rel} 读取失败：${e}`);
    }
  }

  if (failed) {
    console.error('\n驱动资源完整性检查失败。请从原始驱动包恢复文件后再提交或发版。');
    process.exit(1);
  }

  console.log('\n驱动资源完整性检查通过。');
}

void main().catch((e) => {
  console.error(`[资源检查异常] ${e}`);
  process.exit(1);
});
