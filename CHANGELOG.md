# Changelog

## 0.4.2

* `--python` builds again; the layer's check for the pyright language server failed with current pyright releases. `images/` changed, so the python layer is rebuilt once on first start.
* With `--cloud ro`, read-only calls are no longer denied because of a flag value: `kubectl --context ctx -n ns get ns`, `kubectl config view -o yaml`, `helm -n ns list` and `aws ec2 describe-instances --output json` now go through. `images/` changed, so every image is rebuilt once on first start.
* With `--docker`, testcontainers reaches the containers it starts without any configuration: `TESTCONTAINERS_HOST_OVERRIDE=localhost` is exported next to `TESTCONTAINERS_RYUK_DISABLED=true`.

Historical changes have been moved to [OLDER_CHANGES.md](OLDER_CHANGES.md).
