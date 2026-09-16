//! Генерация динамического Ed25519-сертификата REALITY (rkn-fix).

use ::time::{Duration, OffsetDateTime};
use anyhow::{bail, Context, Result};
use rand::RngCore;
use rcgen::{
    CertificateParams, DistinguishedName, DnType, ExtendedKeyUsagePurpose, KeyPair,
    KeyUsagePurpose, SanType, SerialNumber, PKCS_ED25519,
};
use ring::hmac;
use rustls_pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};
use x509_parser::extensions::ParsedExtension;
use x509_parser::oid_registry::{self, Oid};
use x509_parser::prelude::*;

/// Сгенерировать REALITY-сертификат с HMAC-SHA512 подписью.
///
/// `server_name` — SNI клиента (используется как CN при отсутствии ImpersonateCert).
/// `impersonate_der` — опциональный DER leaf-сертификата dest для клонирования метаданных.
pub fn generate_reality_cert(
    auth_key: &[u8; 32],
    server_name: &str,
    impersonate_der: Option<&[u8]>,
) -> Result<(CertificateDer<'static>, PrivateKeyDer<'static>)> {
    let key_pair = KeyPair::generate_for(&PKCS_ED25519)?;
    let pub_key_raw = key_pair.public_key_raw().to_vec();

    let params = if let Some(der) = impersonate_der {
        build_impersonate_params(der, server_name)?
    } else {
        build_fallback_params(server_name)?
    };

    let cert = params.self_signed(&key_pair)?;
    let mut cert_der = cert.der().to_vec();
    let priv_key_der = key_pair.serialize_der();

    patch_reality_signature(&mut cert_der, auth_key, &pub_key_raw)?;

    Ok((
        CertificateDer::from(cert_der),
        PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(priv_key_der)),
    ))
}

fn patch_reality_signature(
    cert_der: &mut [u8],
    auth_key: &[u8; 32],
    pub_key_raw: &[u8],
) -> Result<()> {
    if cert_der.len() < 64 {
        bail!("generated certificate DER is too short");
    }
    let sig_pos = cert_der.len() - 64;
    let ring_key = hmac::Key::new(hmac::HMAC_SHA512, auth_key);
    let signature = hmac::sign(&ring_key, pub_key_raw);
    cert_der[sig_pos..].copy_from_slice(signature.as_ref());
    Ok(())
}

fn build_fallback_params(server_name: &str) -> Result<CertificateParams> {
    let mut rng = rand::thread_rng();
    let mut serial_bytes = [0u8; 16];
    rng.fill_bytes(&mut serial_bytes);
    serial_bytes[0] |= 0x01;

    let mut jitter = [0u8; 2];
    rng.fill_bytes(&mut jitter);
    let days_ago = 30 + (jitter[0] as usize % 60);
    let days_valid = 365 + (jitter[1] as usize % 365);

    let not_before = OffsetDateTime::now_utc() - Duration::days(days_ago as i64);
    let not_after = not_before + Duration::days(days_valid as i64);

    let mut params = CertificateParams::new(vec![server_name.to_string()])?;
    params.serial_number = Some(SerialNumber::from_slice(&serial_bytes));
    params.not_before = not_before;
    params.not_after = not_after;
    params.distinguished_name = DistinguishedName::new();
    params
        .distinguished_name
        .push(DnType::CommonName, server_name);
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ServerAuth];
    Ok(params)
}

fn build_impersonate_params(der: &[u8], server_name: &str) -> Result<CertificateParams> {
    let (_, cert) = X509Certificate::from_der(der).context("parse impersonate cert DER")?;

    let mut params = CertificateParams::default();
    params.serial_number = Some(SerialNumber::from_slice(cert.raw_serial()));
    params.not_before = cert.validity().not_before.to_datetime();
    params.not_after = cert.validity().not_after.to_datetime();
    params.distinguished_name = parse_distinguished_name(cert.subject())?;
    if params.distinguished_name.get(&DnType::CommonName).is_none() {
        params
            .distinguished_name
            .push(DnType::CommonName, server_name);
    }
    params.subject_alt_names = parse_subject_alt_names(&cert)?;
    apply_key_usages(&mut params, &cert);

    if params.subject_alt_names.is_empty() {
        params.subject_alt_names = vec![SanType::DnsName(
            server_name.try_into().context("invalid DNS name in SNI")?,
        )];
    }

    Ok(params)
}

fn parse_distinguished_name(subject: &X509Name) -> Result<DistinguishedName> {
    let mut dn = DistinguishedName::new();
    for rdn in subject.iter() {
        for attr in rdn.iter() {
            let value = attr
                .as_str()
                .context("invalid subject attribute encoding")?;
            if let Some(dn_type) = map_dn_type(attr.attr_type()) {
                dn.push(dn_type, value);
            }
        }
    }
    Ok(dn)
}

fn map_dn_type(oid: &Oid<'_>) -> Option<DnType> {
    if oid == &oid_registry::OID_X509_COMMON_NAME {
        Some(DnType::CommonName)
    } else if oid == &oid_registry::OID_X509_COUNTRY_NAME {
        Some(DnType::CountryName)
    } else if oid == &oid_registry::OID_X509_LOCALITY_NAME {
        Some(DnType::LocalityName)
    } else if oid == &oid_registry::OID_X509_STATE_OR_PROVINCE_NAME {
        Some(DnType::StateOrProvinceName)
    } else if oid == &oid_registry::OID_X509_ORGANIZATION_NAME {
        Some(DnType::OrganizationName)
    } else if oid == &oid_registry::OID_X509_ORGANIZATIONAL_UNIT {
        Some(DnType::OrganizationalUnitName)
    } else {
        None
    }
}

fn parse_subject_alt_names(cert: &X509Certificate<'_>) -> Result<Vec<SanType>> {
    let mut sans = Vec::new();
    for ext in cert.extensions() {
        if let ParsedExtension::SubjectAlternativeName(san) = ext.parsed_extension() {
            for name in &san.general_names {
                match name {
                    GeneralName::DNSName(dns) => {
                        sans.push(SanType::DnsName(
                            (*dns).try_into().context("invalid DNS SAN")?,
                        ));
                    }
                    GeneralName::IPAddress(ip) => {
                        sans.push(SanType::IpAddress(match ip.len() {
                            4 => std::net::IpAddr::from([ip[0], ip[1], ip[2], ip[3]]),
                            16 => {
                                let mut octets = [0u8; 16];
                                octets.copy_from_slice(ip);
                                std::net::IpAddr::from(octets)
                            }
                            _ => continue,
                        }));
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(sans)
}

fn apply_key_usages(params: &mut CertificateParams, cert: &X509Certificate<'_>) {
    let mut usages = Vec::new();
    let mut ext_usages = Vec::new();

    for ext in cert.extensions() {
        match ext.parsed_extension() {
            ParsedExtension::KeyUsage(ku) => {
                if ku.digital_signature() {
                    usages.push(KeyUsagePurpose::DigitalSignature);
                }
                if ku.key_encipherment() {
                    usages.push(KeyUsagePurpose::KeyEncipherment);
                }
            }
            ParsedExtension::ExtendedKeyUsage(eku) => {
                if eku.server_auth {
                    ext_usages.push(ExtendedKeyUsagePurpose::ServerAuth);
                }
            }
            _ => {}
        }
    }

    if usages.is_empty() {
        usages.push(KeyUsagePurpose::DigitalSignature);
    }
    if ext_usages.is_empty() {
        ext_usages.push(ExtendedKeyUsagePurpose::ServerAuth);
    }

    params.key_usages = usages;
    params.extended_key_usages = ext_usages;
}

#[cfg(test)]
mod tests {
    use super::*;
    use rcgen::generate_simple_self_signed;

    #[test]
    fn fallback_certs_have_unique_serials() {
        let auth_key = [7u8; 32];
        let (a, _) = generate_reality_cert(&auth_key, "client.example.com", None).unwrap();
        let (b, _) = generate_reality_cert(&auth_key, "client.example.com", None).unwrap();
        assert_ne!(a.as_ref(), b.as_ref());
    }

    #[test]
    fn fallback_cert_has_hmac_signature_tail() {
        let auth_key = [9u8; 32];
        let (cert, _) = generate_reality_cert(&auth_key, "client.example.com", None).unwrap();
        let der = cert.as_ref();
        assert!(der.len() >= 64);
        let tail = &der[der.len() - 64..];
        assert!(tail.iter().any(|b| *b != 0));
    }

    #[test]
    fn impersonate_uses_template_metadata() {
        let template = generate_simple_self_signed(vec!["dest.example.com".to_string()]).unwrap();
        let template_der = template.cert.der().to_vec();
        let auth_key = [3u8; 32];
        let (cert, _) =
            generate_reality_cert(&auth_key, "client.example.com", Some(&template_der)).unwrap();
        let (_, parsed) = X509Certificate::from_der(cert.as_ref()).unwrap();
        let template_parsed = X509Certificate::from_der(&template_der).unwrap().1;
        assert_eq!(parsed.raw_serial(), template_parsed.raw_serial());
    }
}
