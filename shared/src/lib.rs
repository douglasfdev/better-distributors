use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Product {
    pub name: String,
    pub unit: String,
    pub price: Option<f64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct Store {
    pub id: String,
    pub name: String,
    pub segment: String,
    pub kind: String,
    pub phone: String,
    pub email: String,
    pub website: String,
    pub address: String,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub catalog_url: String,
    pub notes: String,
    pub products: Vec<Product>,
    pub updated_at: String,
}

impl Store {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.name.trim().is_empty() {
            return Err("Informe o nome da loja.");
        }
        if self.name.len() > 250 || self.notes.len() > 10000 || self.products.len() > 500 {
            return Err("Cadastro excede os limites de tamanho.");
        }
        match (self.latitude, self.longitude) {
            (Some(lat), Some(lng))
                if lat.is_finite()
                    && lng.is_finite()
                    && (-90.0..=90.0).contains(&lat)
                    && (-180.0..=180.0).contains(&lng) => {}
            (None, None) => {}
            _ => return Err("Informe latitude e longitude válidas, ou deixe ambas vazias."),
        }
        for value in [
            &self.segment,
            &self.kind,
            &self.phone,
            &self.email,
            &self.website,
            &self.address,
            &self.catalog_url,
        ] {
            if value.len() > 2000 {
                return Err("Campo excede o limite de 2000 caracteres.");
            }
        }
        for url in [&self.website, &self.catalog_url] {
            if !url.is_empty() {
                let parsed = url::Url::parse(url).map_err(|_| "Link inválido.")?;
                if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
                    return Err("Links devem ser endereços HTTP(S) válidos.");
                }
            }
        }
        for item in &self.products {
            if item.name.len() > 250 || item.unit.len() > 100 {
                return Err("Nome ou unidade do produto excede o limite de tamanho.");
            }
            if item.name.trim().is_empty() {
                return Err("Informe o nome de cada produto.");
            }
            if item.price.is_some_and(|p| !p.is_finite() || p < 0.0) {
                return Err("Preço deve ser um número positivo ou zero.");
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn store() -> Store {
        Store {
            name: "Loja".into(),
            ..Default::default()
        }
    }
    #[test]
    fn optional_catalog_and_prices() {
        assert!(store().validate().is_ok());
    }
    #[test]
    fn rejects_partial_location() {
        let mut s = store();
        s.latitude = Some(0.0);
        assert!(s.validate().is_err());
    }
    #[test]
    fn rejects_invalid_coordinates() {
        let mut s = store();
        s.latitude = Some(91.0);
        s.longitude = Some(0.0);
        assert!(s.validate().is_err());
    }
    #[test]
    fn rejects_unsafe_links() {
        let mut s = store();
        s.catalog_url = "javascript:alert(1)".into();
        assert!(s.validate().is_err());
    }
    #[test]
    fn rejects_url_without_host() {
        let mut s = store();
        s.website = "https://".into();
        assert!(s.validate().is_err());
    }
    #[test]
    fn rejects_oversized_product() {
        let mut s = store();
        s.products.push(Product {
            name: "x".repeat(251),
            ..Default::default()
        });
        assert!(s.validate().is_err());
    }
    #[test]
    fn optional_product_price() {
        let mut s = store();
        s.products.push(Product {
            name: "Sabão".into(),
            ..Default::default()
        });
        assert!(s.validate().is_ok());
    }
    #[test]
    fn accepts_zero_coordinates_and_price() {
        let mut s = store();
        s.latitude = Some(0.0);
        s.longitude = Some(0.0);
        s.products.push(Product {
            name: "Amostra".into(),
            price: Some(0.0),
            ..Default::default()
        });
        assert!(s.validate().is_ok());
    }
    #[test]
    fn rejects_non_finite_values() {
        let mut s = store();
        s.latitude = Some(f64::NAN);
        s.longitude = Some(0.0);
        assert!(s.validate().is_err());
        s.latitude = None;
        s.longitude = None;
        s.products.push(Product {
            name: "Item".into(),
            price: Some(f64::INFINITY),
            ..Default::default()
        });
        assert!(s.validate().is_err());
    }
    #[test]
    fn rejects_negative_price() {
        let mut s = store();
        s.products.push(Product {
            name: "Sabão".into(),
            price: Some(-1.0),
            ..Default::default()
        });
        assert!(s.validate().is_err());
    }
}
