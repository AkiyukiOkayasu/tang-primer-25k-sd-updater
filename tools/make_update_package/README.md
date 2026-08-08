# make_update_package

FPGAOSC.UPD を生成する Firmware 用のホスト側ツール。package header と app slot の定義は
[update_common/update_spec.conf](../../update_common/update_spec.conf) を読む。

```sh
cd Firmware
python3 tools/make_update_package/make_update_package.py app.bin FPGAOSC.UPD --app-version 1
```

出力ファイルは docs/sd-update-plan.md の update container header と一致する。
