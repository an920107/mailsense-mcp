use crate::config::PersonalConfig;
use crate::domain::{EmailMessage, PasswordComponent};
use chrono::{Datelike, Duration, NaiveDate, Utc};

pub struct PasswordPoolBuilder<'a> {
    config: &'a PersonalConfig,
}

impl<'a> PasswordPoolBuilder<'a> {
    pub fn new(config: &'a PersonalConfig) -> Self {
        Self { config }
    }

    pub fn build(
        &self,
        email: &EmailMessage,
        recipes: Option<&Vec<Vec<PasswordComponent>>>,
    ) -> Vec<String> {
        let mut pool = Vec::new();

        // 1. Process Recipes from LLM
        if let Some(recipes) = recipes {
            for recipe in recipes {
                if let Some(password) = self.assemble_recipe(recipe) {
                    pool.push(password);
                }
            }
        }

        // 2. Baseline Temporal Brute-Force (+/- 14 days around email date)
        if let Ok(email_date) = chrono::DateTime::parse_from_rfc3339(&email.date) {
            let base_date = email_date.with_timezone(&Utc).date_naive();
            for i in -14..=14 {
                if let Some(date) = base_date.checked_add_signed(Duration::days(i)) {
                    pool.extend(self.generate_date_variants(date));
                }
            }
        }

        // 3. Common Personal Info Combinations
        pool.push(self.config.id_number.clone());
        pool.push(self.config.id_number.to_lowercase());
        pool.push(self.config.birthday.clone());

        // Deduplicate
        pool.sort();
        pool.dedup();
        pool
    }

    fn assemble_recipe(&self, components: &[PasswordComponent]) -> Option<String> {
        let mut password = String::new();
        for comp in components {
            match comp {
                PasswordComponent::Id { operation, length } => {
                    let id = &self.config.id_number;
                    let val = match operation.as_str() {
                        "Full" => id.clone(),
                        "First" => {
                            let len = length.unwrap_or(id.len());
                            id.chars().take(len).collect()
                        }
                        "Last" => {
                            let len = length.unwrap_or(id.len());
                            id.chars()
                                .rev()
                                .take(len)
                                .collect::<String>()
                                .chars()
                                .rev()
                                .collect()
                        }
                        _ => id.clone(),
                    };
                    password.push_str(&val);
                }
                PasswordComponent::Bday { format } => {
                    let bday_str = &self.config.birthday; // Expected YYYYMMDD
                    if let Ok(date) = NaiveDate::parse_from_str(bday_str, "%Y%m%d") {
                        password.push_str(&self.format_date(date, format));
                    }
                }
                PasswordComponent::Literal { value } => {
                    password.push_str(value);
                }
            }
        }
        if password.is_empty() {
            None
        } else {
            Some(password)
        }
    }

    fn generate_date_variants(&self, date: NaiveDate) -> Vec<String> {
        vec![
            date.format("%Y%m%d").to_string(),
            date.format("%m%d").to_string(),
            date.format("%y%m%d").to_string(),
            // Taiwan MinGuo Year
            format!("{:03}{}", date.year() - 1911, date.format("%m%d")),
        ]
    }

    fn format_date(&self, date: NaiveDate, format: &str) -> String {
        match format {
            "YYYYMMDD" => date.format("%Y%m%d").to_string(),
            "MMDD" => date.format("%m%d").to_string(),
            "YYMMDD" => date.format("%y%m%d").to_string(),
            "YYMM" => date.format("%y%m").to_string(),
            "MINGUO" => format!("{:03}{}", date.year() - 1911, date.format("%m%d")),
            _ => date.format("%Y%m%d").to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_config() -> PersonalConfig {
        PersonalConfig {
            id_number: "A123456789".to_string(),
            birthday: "19900101".to_string(),
        }
    }

    #[test]
    fn test_recipe_assembly() {
        let config = mock_config();
        let builder = PasswordPoolBuilder::new(&config);

        // Test ID Last 4 + Bday MMDD
        let recipe = vec![
            PasswordComponent::Id {
                operation: "Last".to_string(),
                length: Some(4),
            },
            PasswordComponent::Bday {
                format: "MMDD".to_string(),
            },
        ];

        let result = builder.assemble_recipe(&recipe).unwrap();
        assert_eq!(result, "67890101");
    }

    #[test]
    fn test_minguo_date_format() {
        let config = mock_config();
        let builder = PasswordPoolBuilder::new(&config);
        let date = NaiveDate::from_ymd_opt(2026, 5, 10).unwrap();

        let variants = builder.generate_date_variants(date);
        assert!(variants.contains(&"1150510".to_string()));
    }
}
