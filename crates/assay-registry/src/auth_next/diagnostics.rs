use crate::error::RegistryError;

pub(super) fn github_oidc_network_error(error: reqwest::Error) -> RegistryError {
    RegistryError::Network {
        message: format!("failed to request GitHub OIDC token: {}", error),
    }
}

pub(super) fn github_oidc_request_failed(
    status: reqwest::StatusCode,
    body: String,
) -> RegistryError {
    RegistryError::Unauthorized {
        message: format!("GitHub OIDC request failed: HTTP {} - {}", status, body),
    }
}

pub(super) fn github_oidc_parse_error(error: reqwest::Error) -> RegistryError {
    RegistryError::InvalidResponse {
        message: format!("failed to parse GitHub OIDC response: {}", error),
    }
}

pub(super) fn token_exchange_network_error(error: reqwest::Error) -> RegistryError {
    RegistryError::Network {
        message: format!("failed to exchange token: {}", error),
    }
}

pub(super) fn token_exchange_unauthorized() -> RegistryError {
    RegistryError::Unauthorized {
        message: "OIDC token exchange failed: unauthorized".to_string(),
    }
}

pub(super) fn token_exchange_failed(status: reqwest::StatusCode, body: String) -> RegistryError {
    RegistryError::Network {
        message: format!("token exchange failed: HTTP {} - {}", status, body),
    }
}

pub(super) fn token_exchange_parse_error(error: reqwest::Error) -> RegistryError {
    RegistryError::InvalidResponse {
        message: format!("failed to parse registry token response: {}", error),
    }
}
