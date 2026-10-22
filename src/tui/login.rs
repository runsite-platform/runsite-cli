use crate::api::Credentials;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LoginTab {
    #[default]
    Password,
    ApiKey,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LoginField {
    Email,
    Password,
    ApiKey,
}

#[derive(Debug, Default)]
pub struct LoginState {
    pub tab: LoginTab,
    pub email: String,
    pub password: String,
    pub api_key: String,
    /// Index into `fields()`.
    pub focus: usize,
    /// Shown above the form, e.g. why the user was sent here.
    pub notice: Option<String>,
    pub error: Option<String>,
    pub submitting: bool,
    /// `Esc` on an empty field leaves input mode, so `q`, `?` and `P` work.
    pub navigating: bool,
}

impl LoginState {
    pub fn with_notice(notice: &str) -> Self {
        Self {
            notice: Some(notice.to_string()),
            ..Default::default()
        }
    }

    pub fn fields(&self) -> &'static [LoginField] {
        match self.tab {
            LoginTab::Password => &[LoginField::Email, LoginField::Password],
            LoginTab::ApiKey => &[LoginField::ApiKey],
        }
    }

    pub fn focused_field(&self) -> LoginField {
        self.fields()[self.focus.min(self.fields().len() - 1)]
    }

    pub fn field_mut(&mut self, field: LoginField) -> &mut String {
        match field {
            LoginField::Email => &mut self.email,
            LoginField::Password => &mut self.password,
            LoginField::ApiKey => &mut self.api_key,
        }
    }

    pub fn switch_tab(&mut self, tab: LoginTab) {
        self.tab = tab;
        self.focus = 0;
        self.error = None;
    }

    pub fn move_focus(&mut self, forward: bool) {
        let count = self.fields().len();
        self.focus = if forward {
            (self.focus + 1) % count
        } else {
            (self.focus + count - 1) % count
        };
    }

    /// `Err` holds the inline validation message.
    pub fn credentials(&self) -> Result<Credentials, &'static str> {
        match self.tab {
            LoginTab::Password => {
                let email = self.email.trim();
                if email.is_empty() || self.password.is_empty() {
                    return Err("Enter your email and password");
                }
                Ok(Credentials::Password {
                    email: email.to_string(),
                    password: self.password.clone(),
                })
            }
            LoginTab::ApiKey => {
                let key = self.api_key.trim();
                if key.is_empty() {
                    return Err("Paste an API key");
                }
                Ok(Credentials::ApiKey(key.to_string()))
            }
        }
    }
}
