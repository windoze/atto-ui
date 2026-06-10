/**
 * Runs every demo in headless mode and fails if any exits non-zero.
 * Used as a deterministic CI/manual smoke check: npm run smoke
 */
import { spawnSync } from 'node:child_process'
import { join } from 'node:path'

const demos = ['01-hello.tsx', '02-counter.tsx', '03-todo-list.tsx', '04-multi-window.tsx', '05-markdown-stream.tsx', '06-theme-switch.tsx', '07-component-gallery.tsx']

let failures = 0
for (const demo of demos) {
  const result = spawnSync('npx', ['tsx', join('src', demo)], {
    cwd: join(__dirname, '..'),
    env: { ...process.env, ATTO_UI_EXAMPLE_HEADLESS: '1' },
    encoding: 'utf8',
  })
  const ok = result.status === 0
  console.log(`${ok ? 'PASS' : 'FAIL'}  ${demo}`)
  if (!ok) {
    failures += 1
    if (result.stderr) console.error(result.stderr.trimEnd())
  }
}

if (failures > 0) {
  console.error(`\n${failures} demo(s) failed`)
  process.exit(1)
}
console.log('\nall demos passed')
