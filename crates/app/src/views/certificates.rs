//! 证书视图：Thread CA 信息、导出、再生成。

use gpui_kit::assets::IconName;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::notification::NotificationType;
use gpui_kit::component::{ActiveTheme as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::{
    AnyElement, Context, IntoElement, ParentElement as _, Styled as _, Window, div, px,
};
use gpui_kit::{InteractiveElement as _, StatefulInteractiveElement as _};
use thread_core::error::AppError;

use crate::app::AppView;
use crate::widgets;

pub fn render(this: &mut AppView, _window: &mut Window, cx: &mut Context<AppView>) -> AnyElement {
    let theme = cx.theme().clone();
    v_flex()
        .flex_1()
        .min_h_0()
        .id("certificates")
        .overflow_y_scroll()
        .gap_4()
        .px_6()
        .py_4()
        .max_w(px(720.))
        .child(widgets::page_header(
            cx,
            tr!(cx, "CA 证书", "CA certificate"),
            tr!(
                cx,
                "Thread 在本机生成一个根 CA，用于解密 HTTPS 流量。应用不会自动安装证书；需要查看 HTTPS 时，手动导出并信任它。",
                "Thread creates a local root CA to decrypt HTTPS. It is not installed for you. Export it and trust it when you need to inspect HTTPS."
            ),
            Vec::<AnyElement>::new(),
        ))
        .child(match &this.certificate.error {
            Some(error) => widgets::notice_line(cx, error).into_any_element(),
            None => match &this.certificate.info {
                None => div()
                    .text_xs()
                    .text_color(theme.muted_foreground)
                    .child(tr!(cx, "加载中…", "Loading…"))
                    .into_any_element(),
                Some(info) => widgets::section(
                    cx,
                    tr!(cx, "根 CA", "Root CA"),
                    tr!(
                        cx,
                        "仅公钥信息；私钥保存在本地数据目录，永不外发。",
                        "Public information only. The private key stays in the local data folder."
                    ),
                    v_flex()
                        .gap_1()
                        .child(widgets::field_row(
                            cx,
                            tr!(cx, "通用名称", "Common name"),
                            info.common_name.clone(),
                        ))
                        .child(widgets::field_row(
                            cx,
                            tr!(cx, "指纹 (SHA-256)", "Fingerprint (SHA-256)"),
                            info.fingerprint_sha256.clone(),
                        ))
                        .child(widgets::field_row(
                            cx,
                            tr!(cx, "创建时间", "Created"),
                            chrono::DateTime::from_timestamp_millis(info.created_at)
                                .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                                .unwrap_or_else(|| "-".into()),
                        ))
                        .child(widgets::field_row(
                            cx,
                            tr!(cx, "证书文件", "Certificate file"),
                            info.cert_path.clone(),
                        ))
                        .child(
                            h_flex()
                                .gap_2()
                                .pt_3()
                                .child(
                                    Button::new("export-ca")
                                        .outline()
                                        .icon(IconName::Download)
                                        .label(tr!(cx, "导出证书（PEM）", "Export certificate (PEM)"))
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.export_ca(window, cx);
                                        })),
                                )
                                .child(
                                    Button::new("regen-ca")
                                        .outline()
                                        .danger()
                                        .icon(IconName::RefreshCw)
                                        .label(tr!(cx, "重新生成", "Regenerate"))
                                        .tooltip(tr!(
                                            cx,
                                            "所有已信任此 CA 的客户端都需要重新信任新证书",
                                            "Clients that trusted this CA must trust the new certificate"
                                        ))
                                        .on_click(cx.listener(|this, _, window, cx| {
                                            this.regenerate_ca(window, cx);
                                        })),
                                ),
                        ),
                )
                .into_any_element(),
            },
        })
        .child(widgets::section(
            cx,
            tr!(cx, "信任步骤", "How to trust it"),
            tr!(
                cx,
                "以 macOS 为例；Windows 与 Linux 需要把证书导入对应系统或浏览器存储。",
                "macOS is shown below. On Windows and Linux, import the certificate into the system or browser store."
            ),
            v_flex()
                .gap_1()
                .child(widgets::field_row(
                    cx,
                    tr!(cx, "1. 导出", "1. Export"),
                    tr!(cx, "得到 thread-ca.pem", "Save thread-ca.pem"),
                ))
                .child(widgets::field_row(
                    cx,
                    tr!(cx, "2. 加入钥匙串", "2. Add to Keychain"),
                    tr!(
                        cx,
                        "打开「钥匙串访问」→ 导入，并设为「始终信任」",
                        "Open Keychain Access, import it, and set it to Always Trust"
                    ),
                ))
                .child(widgets::field_row(
                    cx,
                    tr!(cx, "3. 启用解密", "3. Turn on decryption"),
                    tr!(
                        cx,
                        "回到代理页打开「解密 HTTPS」，并在下方确认已信任",
                        "On the Proxy page, turn on Decrypt HTTPS and confirm trust below"
                    ),
                ))
                .child(h_flex().gap_2().pt_2().child(
                    Button::new("ack-trust")
                        .outline()
                        .label(if this.proxy.settings.ca_trust_acknowledged {
                            tr!(cx, "已确认信任 CA（点击撤销）", "CA trust confirmed (click to revoke)")
                        } else {
                            tr!(cx, "我已在系统中信任 Thread CA", "I have trusted the Thread CA")
                        })
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.proxy.settings.ca_trust_acknowledged =
                                !this.proxy.settings.ca_trust_acknowledged;
                            this.save_proxy_settings(window, cx);
                        })),
                )),
        ))
        .child(widgets::section(
            cx,
            tr!(cx, "取消信任 / 卸载", "Remove trust"),
            tr!(
                cx,
                "应用不会改系统信任库。删除证书需要你在系统或浏览器里手动操作。重新生成 CA 不会卸掉旧证书。",
                "Thread does not change the system trust store. Remove the certificate yourself. Regenerating the CA does not uninstall the old one."
            ),
            v_flex()
                .gap_1()
                .child(widgets::field_row(
                    cx,
                    "macOS",
                    tr!(
                        cx,
                        "钥匙串访问 → 搜索 Thread → 删除该证书 → 重启浏览器",
                        "Keychain Access → search Thread → delete the certificate → restart the browser"
                    ),
                ))
                .child(widgets::field_row(
                    cx,
                    "Windows",
                    tr!(
                        cx,
                        "certmgr.msc → 受信任的根证书颁发机构 → 删除 Thread CA",
                        "certmgr.msc → Trusted Root Certification Authorities → delete the Thread CA"
                    ),
                ))
                .child(widgets::field_row(
                    cx,
                    "Linux",
                    tr!(
                        cx,
                        "从 nss / ca-certificates 移除后执行 update-ca-certificates，并清掉浏览器里的根证书",
                        "Remove it from nss or ca-certificates, run update-ca-certificates, and clear the browser root"
                    ),
                ))
                .child(widgets::field_row(
                    cx,
                    tr!(cx, "之后", "Then"),
                    tr!(
                        cx,
                        "回到本页点「撤销」确认，代理会停止 MITM，CONNECT 只做隧道转发",
                        "Come back here and revoke the confirmation. The proxy stops MITM and CONNECT stays a tunnel"
                    ),
                )),
        ))
        .into_any_element()
}

impl AppView {
    pub fn reload_certificate(&mut self, cx: &mut Context<Self>) {
        let core = self.core.clone();
        let ca = core.proxy.ca().clone();
        let receiver = core.spawn(async move { ca.info() });
        cx.spawn(async move |this, cx| {
            let result = receiver
                .await
                .map_err(|error| AppError::Internal(error.to_string()))
                .and_then(|outcome| outcome);
            let _ = this.update(cx, |view, cx| {
                match result {
                    Ok(info) => {
                        view.certificate.info = Some(info);
                        view.certificate.error = None;
                    }
                    Err(error) => {
                        view.certificate.error = Some(error.to_string());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn export_ca(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let directory = std::path::PathBuf::from(".");
        let receiver = cx.prompt_for_new_path(&directory, Some("thread-ca.pem"));
        let core = self.core.clone();
        let ca = core.proxy.ca().clone();
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(path))) = receiver.await else {
                return;
            };
            let outcome = ca
                .export_to(&path)
                .map_err(|error| AppError::Internal(error.to_string()));
            let _ = this.update_in(cx, |view, window, cx| {
                match outcome {
                    Ok(path) => window.push_notification(
                        (
                            NotificationType::Success,
                            tr!(
                                cx,
                                format!("证书已导出：{}", path.display()),
                                format!("Certificate exported to {}", path.display())
                            ),
                        ),
                        cx,
                    ),
                    Err(error) => {
                        window.push_notification((NotificationType::Error, error.to_string()), cx)
                    }
                }
                let _ = &view;
            });
        })
        .detach();
    }

    pub fn regenerate_ca(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let core = self.core.clone();
        let ca = core.proxy.ca().clone();
        let receiver = core.spawn(async move { ca.regenerate() });
        cx.spawn(async move |this, cx| {
            let result = receiver
                .await
                .map_err(|error| AppError::Internal(error.to_string()))
                .and_then(|outcome| outcome);
            let _ = this.update_in(cx, |view, window, cx| {
                match result {
                    Ok(info) => {
                        view.certificate.info = Some(info);
                        view.proxy.settings.ca_trust_acknowledged = false;
                        let _ = view
                            .core
                            .config
                            .set_proxy_capture_settings(&view.proxy.settings);
                        window.push_notification(
                            (
                                NotificationType::Warning,
                                tr!(
                                    cx,
                                    "CA 已重新生成，请重新导出并在系统中信任新证书。",
                                    "CA regenerated. Export it again and trust the new certificate."
                                ),
                            ),
                            cx,
                        );
                    }
                    Err(error) => {
                        window.push_notification((NotificationType::Error, error.to_string()), cx)
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}
