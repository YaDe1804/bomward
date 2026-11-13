# Releasing 0.1.0

Deniz does these steps after the release branch is merged. The tag is
created only then. [`.github/workflows/release.yml`](../.github/workflows/release.yml)
builds the three archives and `SHA256SUMS` when `v0.1.0` is pushed.

1. Merge the branch.
2. On the merge commit, set the tag and push it:

   ```text
   git tag v0.1.0 && git push origin v0.1.0
   ```

   Do this only after the merge, never before.
3. The release workflow runs and creates the GitHub Release with the
   assets.
4. Optionally set a moving tag `v0` on that same commit, so a workflow
   can pin `@v0`:

   ```text
   git tag v0 && git push origin v0
   ```
5. In the release dialog, or in the repository settings, publish the
   Action to the GitHub Marketplace. Tick "Publish this Action to the
   GitHub Marketplace" and accept the Developer Agreement. This can
   only be done in the GitHub UI.
6. After the release, check a foreign run. In another repository, use
   the Action at `@v0.1.0` and open a pull request. The download and
   the checksum check must succeed.
