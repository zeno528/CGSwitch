use super::{
    app_err, atomic_write, backup_file, normalize_auth_override, now_ms, parse_external_auth_json,
    read_optional_text, AppContext, AppResult, ManagedAccount, ProfileKind,
};

impl AppContext {
    /// 把订阅凭据原文写入 ~/.codex/auth.json（写前备份旧文件）。
    pub fn write_codex_auth_json(&self, content: &str) -> AppResult<()> {
        let destination = self.paths.codex_home.join("auth.json");
        backup_file(&destination, &self.paths.codex_files_backup, "auth")?;
        atomic_write(&destination, content.as_bytes())?;
        Ok(())
    }

    /// 识别 Codex 官方外部认证（~/.codex/auth.json，由 codex login 生成）。
    /// 只读识别、不导入数据库；不是有效的 ChatGPT 订阅认证时返回 None。
    pub fn external_codex_auth(&self) -> AppResult<Option<ManagedAccount>> {
        let Some(text) = read_optional_text(&self.paths.codex_home.join("auth.json")) else {
            return Ok(None);
        };
        let Some(auth) = parse_external_auth_json(&text) else {
            return Ok(None);
        };
        Ok(Some(ManagedAccount {
            id: auth.account_id,
            login: auth
                .email
                .unwrap_or_else(|| "ChatGPT（Codex 官方认证）".to_string()),
            authenticated_at: 0,
            is_default: false,
        }))
    }

    /// 读取 live auth.json 中有效的 ChatGPT 订阅 access_token（外部 Codex 认证）。
    pub fn external_codex_access_token(&self) -> AppResult<Option<String>> {
        let Some(text) = read_optional_text(&self.paths.codex_home.join("auth.json")) else {
            return Ok(None);
        };
        Ok(parse_external_auth_json(&text).map(|auth| auth.access_token))
    }

    /// 读取 live auth.json 中属于指定账号的 ChatGPT access_token。
    pub fn external_codex_access_token_for_account(
        &self,
        account_id: &str,
    ) -> AppResult<Option<String>> {
        let Some(text) = read_optional_text(&self.paths.codex_home.join("auth.json")) else {
            return Ok(None);
        };
        let Some(auth) = parse_external_auth_json(&text) else {
            return Ok(None);
        };
        Ok((auth.account_id == account_id).then_some(auth.access_token))
    }

    /// 是否为官方订阅供应商（无 API 供应商，凭据走 ChatGPT 订阅）。
    pub fn is_subscription_profile(&self, id: &str) -> AppResult<bool> {
        Ok(self.database.profile(id)?.kind == ProfileKind::Official)
    }

    /// 官方供应商绑定的订阅账号；未绑定返回 None（由调用方回退默认账号）。
    pub fn bound_account_id(&self, id: &str) -> AppResult<Option<String>> {
        Ok(self.database.profile(id)?.account_id.clone())
    }

    pub(super) fn active_profile_state(&self) -> AppResult<Option<String>> {
        Ok(self.database.app_state()?.0)
    }

    /// 该供应商是否为当前使用中（以显式激活状态为准，不做配置比对）。
    pub fn is_active_profile(&self, id: &str) -> AppResult<bool> {
        Ok(self.active_profile_state()?.as_deref() == Some(id))
    }

    /// 官方供应商是否保存了自己的 auth.json 覆盖（有则应用时不再用账号现生成）。
    pub fn has_auth_override(&self, id: &str) -> AppResult<bool> {
        Ok(
            normalize_auth_override(self.database.profile(id)?.payload.raw_auth.as_deref())
                .is_some(),
        )
    }

    /// 官方供应商绑定订阅账号；第三方供应商直接拒绝。None 表示跟随默认账号。
    pub fn set_profile_account(&self, id: &str, account_id: Option<&str>) -> AppResult<()> {
        let stored = self.database.profile(id)?;
        if stored.kind != ProfileKind::Official {
            return Err(app_err!("第三方供应商不支持绑定订阅账号"));
        }
        if let Some(account_id) = account_id {
            if !self
                .database
                .accounts()?
                .iter()
                .any(|account| account.id == account_id)
            {
                return Err(app_err!("订阅账号不存在"));
            }
        }
        self.database
            .set_profile_account(id, account_id, &now_ms().to_string())
    }
}
