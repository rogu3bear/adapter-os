/**
 * Demo operation registry.
 *
 * Re-exports `meta` from each operation spec so they can be enumerated
 * programmatically (e.g. for generating a table of contents or running
 * a subset by tag).
 *
 * ## Adding a new operation
 *
 * 1. Copy `_skeleton.spec.ts` to `<name>.spec.ts`
 * 2. Update `meta` (id, title, mocks, tags)
 * 3. Implement the test body and `run()` function
 * 4. Add a re-export line below:
 *    `export { meta as <name>Meta } from './<name>.spec';`
 * 5. Run: `npm run demo -- <name>` to verify
 */

export { meta as skeletonMeta } from './_skeleton.spec';
export { meta as createAdapterMeta } from './create-adapter.spec';
export { meta as trustNativeMeta } from './trust-native.spec';
export { meta as replaySignedVerifyMeta } from './replay-signed-verify.spec';
