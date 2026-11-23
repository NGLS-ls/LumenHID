//! Local TLS HTTP/2 gRPC client for Edifier HECATE G5 Max THX Audio Service
//! (`thx_hecate_g5max_service` via `C:\ProgramData\THX\hecate_g5max\thx_properties.tcp`).
//!
//! Uses hand-written `prost::Message` structs (no `protoc` required) and a pinned
//! localhost-only `ServerCertVerifier` because the local THX service issues a
//! `CN=localhost` certificate without a SubjectAltName extension.

use base64::engine::general_purpose::STANDARD as BASE64_STANDARD;
use base64::Engine;
use bytes::{BufMut, Bytes, BytesMut};
use prost::Message;
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::crypto::WebPkiSupportedAlgorithms;
use rustls::pki_types::{CertificateDer, ServerName, SignatureVerificationAlgorithm, UnixTime};
use rustls::{
    CertificateError, DigitallySignedStruct, Error as RustlsError, PeerMisbehaved, SignatureScheme,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::net::TcpStream;
use tokio::time::timeout;
use tokio_rustls::TlsConnector;

pub const THX_TCP_CONFIG_PATH: &str = r"C:\ProgramData\THX\hecate_g5max\thx_properties.tcp";
pub const THX_ENDPOINT_HEADPHONES: &str = "headphones";
const RPC_TIMEOUT: Duration = Duration::from_secs(3);

pub const PROPERTY_TYPE_BOOL: i32 = 1;
#[allow(dead_code)]
pub const PROPERTY_TYPE_INT: i32 = 2;
pub const PROPERTY_TYPE_FLOAT: i32 = 3;
#[allow(dead_code)]
pub const PROPERTY_TYPE_STRING: i32 = 4;
#[allow(dead_code)]
pub const PROPERTY_TYPE_STRING_ARRAY: i32 = 5;
#[allow(dead_code)]
pub const PROPERTY_TYPE_FLOAT_ARRAY: i32 = 6;

#[derive(Clone, PartialEq, Message)]
pub struct GetVersionRequest {}

#[derive(Clone, PartialEq, Message)]
pub struct GetVersionResponse {
    #[prost(string, tag = "1")]
    pub version: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct GetEndpointListRequest {
    #[prost(string, tag = "1")]
    pub r#type: String,
}

#[derive(Clone, PartialEq, Message)]
pub struct GetEndpointListResponse {
    #[prost(string, repeated, tag = "1")]
    pub endpoint_ids: Vec<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct GetPropertyRequest {
    #[prost(string, tag = "1")]
    pub id: String,
    #[prost(string, optional, tag = "2")]
    pub endpoint_id: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct ThxProperty {
    #[prost(string, tag = "1")]
    pub id: String,
    #[prost(int32, tag = "2")]
    pub r#type: i32,
    #[prost(float, optional, tag = "3")]
    pub float_value: Option<f32>,
    #[prost(int64, optional, tag = "4")]
    pub int_value: Option<i64>,
    #[prost(bool, optional, tag = "5")]
    pub bool_value: Option<bool>,
    #[prost(string, optional, tag = "6")]
    pub string_value: Option<String>,
    #[prost(string, repeated, tag = "7")]
    pub string_array_value: Vec<String>,
    #[prost(float, repeated, tag = "8")]
    pub float_array_value: Vec<f32>,
}

#[derive(Clone, PartialEq, Message)]
pub struct SetPropertyRequest {
    #[prost(message, optional, tag = "1")]
    pub property: Option<ThxProperty>,
    #[prost(string, optional, tag = "2")]
    pub endpoint_id: Option<String>,
}

#[derive(Clone, PartialEq, Message)]
pub struct Empty {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThxStatus {
    pub available: bool,
    pub version: Option<String>,
    pub endpoint: Option<String>,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ThxAudioSnapshot {
    pub eq_enabled: bool,
    pub eq_bands: [f32; 10],
    pub spatial_enabled: bool,
    pub upmix_enabled: bool,
    pub width_expansion_enabled: bool,
    pub bassboost_enabled: bool,
    pub bassboost_level: f32,
    pub voice_clarity_enabled: bool,
    pub voice_clarity_level: f32,
    pub volume_leveling_enabled: bool,
    pub volume_leveling_level: f32,
}

impl Default for ThxAudioSnapshot {
    fn default() -> Self {
        Self {
            eq_enabled: false,
            eq_bands: [0.0; 10],
            spatial_enabled: false,
            upmix_enabled: false,
            width_expansion_enabled: false,
            bassboost_enabled: false,
            bassboost_level: 0.55,
            voice_clarity_enabled: false,
            voice_clarity_level: 0.0,
            volume_leveling_enabled: false,
            volume_leveling_level: 0.0,
        }
    }
}

#[derive(Debug, Deserialize)]
struct ThxTcpDiscoveryRaw {
    address: String,
    cert: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThxDiscoveryConfig {
    pub host: String,
    pub port: u16,
    pub cert_der: Vec<u8>,
}

/// Decodes a PEM-encoded X.509 certificate into raw DER bytes.
pub fn parse_pem_certificate_der(pem: &str) -> Result<Vec<u8>, String> {
    const BEGIN: &str = "-----BEGIN CERTIFICATE-----";
    const END: &str = "-----END CERTIFICATE-----";
    let start = pem
        .find(BEGIN)
        .ok_or_else(|| "THX 证书缺少 BEGIN CERTIFICATE 标记".to_string())?
        + BEGIN.len();
    let end = pem[start..]
        .find(END)
        .ok_or_else(|| "THX 证书缺少 END CERTIFICATE 标记".to_string())?;
    let b64: String = pem[start..start + end]
        .chars()
        .filter(|c| !c.is_ascii_whitespace())
        .collect();
    if b64.is_empty() {
        return Err("THX 证书 PEM 内容为空".to_string());
    }
    BASE64_STANDARD
        .decode(b64)
        .map_err(|e| format!("THX 证书 Base64 解码失败: {e}"))
}

/// Validates and parses `thx_properties.tcp` JSON content, strictly permitting loopback hosts only.
pub fn parse_discovery_json(raw: &str) -> Result<ThxDiscoveryConfig, String> {
    let parsed: ThxTcpDiscoveryRaw =
        serde_json::from_str(raw).map_err(|e| format!("解析 thx_properties.tcp 失败: {e}"))?;
    let addr = parsed.address.trim();

    let (host, port_str) = if let Some(rest) = addr.strip_prefix('[') {
        let (h, p) = rest
            .split_once("]:")
            .ok_or_else(|| format!("非法 THX 服务地址格式: {addr}"))?;
        (h.trim(), p.trim())
    } else {
        addr.rsplit_once(':')
            .ok_or_else(|| format!("THX 服务地址缺少端口: {addr}"))?
    };

    let host_lower = host.to_ascii_lowercase();
    if !matches!(host_lower.as_str(), "localhost" | "127.0.0.1" | "::1") {
        return Err(format!(
            "安全策略拦截：THX gRPC 仅允许连接本机回环地址，拒绝 {host}"
        ));
    }

    let port: u16 = port_str
        .parse()
        .map_err(|_| format!("THX 服务端口号无效: {port_str}"))?;
    let cert_der = parse_pem_certificate_der(&parsed.cert)?;

    Ok(ThxDiscoveryConfig {
        host: host_lower,
        port,
        cert_der,
    })
}

pub fn load_thx_discovery_config() -> Result<ThxDiscoveryConfig, String> {
    let path = Path::new(THX_TCP_CONFIG_PATH);
    if !path.exists() {
        return Err("未检测到 C:\\ProgramData\\THX\\hecate_g5max\\thx_properties.tcp（请确认已安装 HECATE / THX 驱动服务）".to_string());
    }
    let raw = fs::read_to_string(path).map_err(|e| format!("读取 THX 服务配置失败: {e}"))?;
    parse_discovery_json(&raw)
}

/// Parses one ASN.1 DER TLV element, returning `(tag, inner_value, remaining_bytes)`.
fn parse_der_tlv(input: &[u8]) -> Result<(u8, &[u8], &[u8]), String> {
    if input.len() < 2 {
        return Err("ASN.1 DER 数据过短".to_string());
    }
    let tag = input[0];
    let len_byte = input[1];
    let (len, offset) = if (len_byte & 0x80) == 0 {
        (len_byte as usize, 2usize)
    } else {
        let num_octets = (len_byte & 0x7F) as usize;
        if num_octets == 0 || num_octets > 4 || input.len() < 2 + num_octets {
            return Err("ASN.1 DER 长度字段无效".to_string());
        }
        let mut len_val = 0usize;
        for &b in &input[2..2 + num_octets] {
            len_val = (len_val << 8) | (b as usize);
        }
        (len_val, 2 + num_octets)
    };
    let end = offset
        .checked_add(len)
        .ok_or_else(|| "ASN.1 DER 长度溢出".to_string())?;
    if input.len() < end {
        return Err("ASN.1 DER 元素截断".to_string());
    }
    Ok((tag, &input[offset..end], &input[end..]))
}

/// Extracts `(algorithm_identifier_inner_bytes, subject_public_key_bytes)` from an X.509 v1 or v3 DER certificate.
pub fn extract_spki_from_x509_der(cert_der: &[u8]) -> Result<(Vec<u8>, Vec<u8>), String> {
    let (cert_tag, cert_inner, _) = parse_der_tlv(cert_der)?;
    if cert_tag != 0x30 {
        return Err("X.509 根结构非 SEQUENCE".to_string());
    }
    let (tbs_tag, mut cursor, _) = parse_der_tlv(cert_inner)?;
    if tbs_tag != 0x30 {
        return Err("TBSCertificate 非 SEQUENCE".to_string());
    }

    // Optional explicit [0] version tag: present in X.509 v2/v3, omitted in X.509 v1.
    if cursor.first() == Some(&0xA0) {
        let (_, _, rest) = parse_der_tlv(cursor)?;
        cursor = rest;
    }

    // Advance past: serialNumber (0x02), signature (0x30), issuer (0x30), validity (0x30), subject (0x30).
    let expected_tags = [0x02u8, 0x30, 0x30, 0x30, 0x30];
    for expected in expected_tags {
        let (tag, _, rest) = parse_der_tlv(cursor)?;
        if tag != expected {
            return Err(format!(
                "X.509 TBSCertificate 字段标签不匹配：期望 0x{expected:02X}，实际 0x{tag:02X}"
            ));
        }
        cursor = rest;
    }

    let (spki_tag, spki_inner, _) = parse_der_tlv(cursor)?;
    if spki_tag != 0x30 {
        return Err("SubjectPublicKeyInfo 非 SEQUENCE".to_string());
    }
    let (alg_tag, alg_inner, spki_rest) = parse_der_tlv(spki_inner)?;
    if alg_tag != 0x30 {
        return Err("SPKI AlgorithmIdentifier 非 SEQUENCE".to_string());
    }
    let (pk_tag, pk_bits, _) = parse_der_tlv(spki_rest)?;
    if pk_tag != 0x03 || pk_bits.is_empty() || pk_bits[0] != 0x00 {
        return Err("SPKI subjectPublicKey BIT STRING 格式无效".to_string());
    }

    Ok((alg_inner.to_vec(), pk_bits[1..].to_vec()))
}

#[derive(Debug)]
struct PinnedLocalhostCertVerifier {
    expected_der: Vec<u8>,
    spki_alg_id: Vec<u8>,
    spki_public_key: Vec<u8>,
    supported: WebPkiSupportedAlgorithms,
}

impl PinnedLocalhostCertVerifier {
    fn new(expected_der: Vec<u8>) -> Result<Self, String> {
        let (spki_alg_id, spki_public_key) = extract_spki_from_x509_der(&expected_der)?;
        Ok(Self {
            expected_der,
            spki_alg_id,
            spki_public_key,
            supported: rustls::crypto::ring::default_provider().signature_verification_algorithms,
        })
    }

    fn verify_pinned_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
        require_tls13: bool,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        if cert.as_ref() != self.expected_der.as_slice() {
            return Err(RustlsError::InvalidCertificate(
                CertificateError::UnknownIssuer,
            ));
        }

        if require_tls13
            && !matches!(
                dss.scheme,
                SignatureScheme::RSA_PSS_SHA256
                    | SignatureScheme::RSA_PSS_SHA384
                    | SignatureScheme::RSA_PSS_SHA512
                    | SignatureScheme::ECDSA_NISTP256_SHA256
                    | SignatureScheme::ECDSA_NISTP384_SHA384
                    | SignatureScheme::ECDSA_NISTP521_SHA512
                    | SignatureScheme::ED25519
                    | SignatureScheme::ED448
            )
        {
            return Err(PeerMisbehaved::SignedHandshakeWithUnadvertisedSigScheme.into());
        }

        let Some((_, algs)) = self
            .supported
            .mapping
            .iter()
            .find(|(scheme, _)| *scheme == dss.scheme)
        else {
            return Err(PeerMisbehaved::SignedHandshakeWithUnadvertisedSigScheme.into());
        };

        let mut attempted = false;
        for alg in *algs {
            let alg_ref: &dyn SignatureVerificationAlgorithm = *alg;
            if alg_ref.public_key_alg_id().as_ref() != self.spki_alg_id.as_slice() {
                continue;
            }
            attempted = true;
            if alg_ref
                .verify_signature(&self.spki_public_key, message, dss.signature())
                .is_ok()
            {
                return Ok(HandshakeSignatureValid::assertion());
            }
        }

        if !attempted {
            return Err(PeerMisbehaved::SignedHandshakeWithUnadvertisedSigScheme.into());
        }
        Err(RustlsError::InvalidCertificate(
            CertificateError::BadSignature,
        ))
    }
}

impl ServerCertVerifier for PinnedLocalhostCertVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, RustlsError> {
        let is_local_name = match server_name {
            ServerName::DnsName(dns) => dns.as_ref().eq_ignore_ascii_case("localhost"),
            ServerName::IpAddress(ip) => std::net::IpAddr::from(*ip).is_loopback(),
            _ => false,
        };
        if !is_local_name {
            return Err(RustlsError::InvalidCertificate(
                CertificateError::NotValidForName,
            ));
        }
        if end_entity.as_ref() != self.expected_der.as_slice() {
            return Err(RustlsError::InvalidCertificate(
                CertificateError::UnknownIssuer,
            ));
        }
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        self.verify_pinned_signature(message, cert, dss, false)
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        self.verify_pinned_signature(message, cert, dss, true)
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.supported.supported_schemes()
    }
}

/// Packs a protobuf payload into a 5-byte uncompressed gRPC frame.
pub fn pack_grpc_frame<M: Message>(msg: &M) -> Result<Bytes, String> {
    let encoded_len = msg.encoded_len();
    let len_u32 = u32::try_from(encoded_len).map_err(|_| "gRPC 请求报文过大".to_string())?;
    let mut buf = BytesMut::with_capacity(5 + encoded_len);
    buf.put_u8(0);
    buf.put_u32(len_u32);
    msg.encode(&mut buf)
        .map_err(|e| format!("Protobuf 编码失败: {e}"))?;
    Ok(buf.freeze())
}

/// Unpacks a single 5-byte uncompressed gRPC frame from response bytes.
pub fn unpack_grpc_frame(raw: &[u8]) -> Result<&[u8], String> {
    if raw.len() < 5 {
        return Err(format!("gRPC 响应帧长度不足 5 字节（实际 {}）", raw.len()));
    }
    if raw[0] != 0 {
        return Err(format!("不支持的 gRPC 压缩标志: {}", raw[0]));
    }
    let msg_len = u32::from_be_bytes([raw[1], raw[2], raw[3], raw[4]]) as usize;
    let total = 5usize
        .checked_add(msg_len)
        .ok_or_else(|| "gRPC 响应帧长度溢出".to_string())?;
    if raw.len() < total {
        return Err(format!(
            "gRPC 响应帧截断：声明 {msg_len} 字节，实际仅 {} 字节",
            raw.len() - 5
        ));
    }
    Ok(&raw[5..total])
}

pub struct ThxClient {
    port: u16,
    sender: h2::client::SendRequest<Bytes>,
}

impl ThxClient {
    pub async fn connect() -> Result<Self, String> {
        let cfg = load_thx_discovery_config()?;
        Self::connect_with_config(&cfg).await
    }

    pub async fn connect_with_config(cfg: &ThxDiscoveryConfig) -> Result<Self, String> {
        let verifier = PinnedLocalhostCertVerifier::new(cfg.cert_der.clone())?;
        let mut tls_cfg = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .map_err(|e| format!("初始化 TLS 协议版本失败: {e}"))?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(verifier))
        .with_no_client_auth();
        tls_cfg.alpn_protocols = vec![b"h2".to_vec()];

        let connect_target = if cfg.host == "::1" {
            format!("[::1]:{}", cfg.port)
        } else {
            format!("{}:{}", cfg.host, cfg.port)
        };

        let tcp = timeout(RPC_TIMEOUT, TcpStream::connect(&connect_target))
            .await
            .map_err(|_| format!("连接本地 THX 服务超时（{connect_target}）"))?
            .map_err(|e| format!("无法连接本地 THX 服务（{connect_target}）: {e}"))?;

        let server_name = ServerName::try_from("localhost".to_string())
            .map_err(|e| format!("无效 SNI 主机名: {e}"))?;
        let connector = TlsConnector::from(Arc::new(tls_cfg));
        let tls_stream = timeout(RPC_TIMEOUT, connector.connect(server_name, tcp))
            .await
            .map_err(|_| "THX 服务 TLS 握手超时".to_string())?
            .map_err(|e| format!("THX 服务 TLS 握手失败: {e}"))?;

        let (sender, connection) = timeout(RPC_TIMEOUT, h2::client::handshake(tls_stream))
            .await
            .map_err(|_| "THX 服务 HTTP/2 握手超时".to_string())?
            .map_err(|e| format!("THX 服务 HTTP/2 握手失败: {e}"))?;

        tokio::spawn(async move {
            let _ = connection.await;
        });

        Ok(Self {
            port: cfg.port,
            sender,
        })
    }

    async fn unary<Req: Message, Resp: Message + Default>(
        &mut self,
        method: &str,
        req: &Req,
    ) -> Result<Resp, String> {
        let framed = pack_grpc_frame(req)?;
        timeout(RPC_TIMEOUT, self.unary_inner(method, framed))
            .await
            .map_err(|_| format!("THX gRPC 调用 {method} 超时（>3s）"))?
    }

    async fn unary_inner<Resp: Message + Default>(
        &mut self,
        method: &str,
        framed: Bytes,
    ) -> Result<Resp, String> {
        let mut ready_sender = self
            .sender
            .clone()
            .ready()
            .await
            .map_err(|e| format!("THX gRPC HTTP/2 通道未就绪: {e}"))?;

        let uri = format!(
            "https://localhost:{}/thx.proto.ThxAudioProperties/{method}",
            self.port
        );
        let request = http::Request::builder()
            .method(http::Method::POST)
            .uri(uri)
            .header("content-type", "application/grpc")
            .header("te", "trailers")
            .body(())
            .map_err(|e| format!("构造 gRPC HTTP 请求失败: {e}"))?;

        let (resp_future, mut send_stream) = ready_sender
            .send_request(request, false)
            .map_err(|e| format!("发送 gRPC 请求头失败: {e}"))?;
        send_stream
            .send_data(framed, true)
            .map_err(|e| format!("发送 gRPC 请求体失败: {e}"))?;

        let resp = resp_future
            .await
            .map_err(|e| format!("等待 gRPC 响应失败: {e}"))?;
        let http_status = resp.status();
        if http_status != http::StatusCode::OK {
            return Err(format!("THX gRPC HTTP 响应状态异常: {http_status}"));
        }

        let header_status = resp
            .headers()
            .get("grpc-status")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let header_message = resp
            .headers()
            .get("grpc-message")
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);

        let mut body = resp.into_body();
        let mut raw_body = Vec::new();
        while let Some(chunk_res) = body.data().await {
            let chunk = chunk_res.map_err(|e| format!("读取 gRPC 响应流失败: {e}"))?;
            let _ = body.flow_control().release_capacity(chunk.len());
            raw_body.extend_from_slice(&chunk);
        }

        let trailers = body
            .trailers()
            .await
            .map_err(|e| format!("读取 gRPC trailers 失败: {e}"))?;

        let trailer_status = trailers
            .as_ref()
            .and_then(|t| t.get("grpc-status"))
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);
        let trailer_message = trailers
            .as_ref()
            .and_then(|t| t.get("grpc-message"))
            .and_then(|v| v.to_str().ok())
            .map(str::to_string);

        let grpc_status = trailer_status
            .or(header_status)
            .ok_or_else(|| "THX gRPC 响应缺少 grpc-status 状态码".to_string())?;

        if grpc_status != "0" {
            let msg = trailer_message
                .or(header_message)
                .unwrap_or_else(|| "无详细错误信息".to_string());
            return Err(format!(
                "THX gRPC {method} 返回错误 grpc-status={grpc_status}: {msg}"
            ));
        }

        if raw_body.is_empty() {
            return Ok(Resp::default());
        }

        let payload = unpack_grpc_frame(&raw_body)?;
        Resp::decode(payload).map_err(|e| format!("THX gRPC Protobuf 解码失败: {e}"))
    }

    pub async fn get_version(&mut self) -> Result<String, String> {
        let resp: GetVersionResponse = self.unary("GetVersion", &GetVersionRequest {}).await?;
        Ok(resp.version)
    }

    pub async fn get_endpoints(&mut self, kind: &str) -> Result<Vec<String>, String> {
        let resp: GetEndpointListResponse = self
            .unary(
                "GetEndpointList",
                &GetEndpointListRequest {
                    r#type: kind.to_string(),
                },
            )
            .await?;
        Ok(resp.endpoint_ids)
    }

    pub async fn get_property(
        &mut self,
        endpoint_id: &str,
        id: &str,
    ) -> Result<ThxProperty, String> {
        self.unary(
            "GetProperty",
            &GetPropertyRequest {
                id: id.to_string(),
                endpoint_id: Some(endpoint_id.to_string()),
            },
        )
        .await
    }

    pub async fn set_property(
        &mut self,
        endpoint_id: &str,
        property: ThxProperty,
    ) -> Result<(), String> {
        let _empty: Empty = self
            .unary(
                "SetProperty",
                &SetPropertyRequest {
                    property: Some(property),
                    endpoint_id: Some(endpoint_id.to_string()),
                },
            )
            .await?;
        Ok(())
    }

    pub async fn capture_headphones_snapshot(&mut self) -> Result<ThxAudioSnapshot, String> {
        let mut snap = ThxAudioSnapshot {
            eq_enabled: self
                .get_property(THX_ENDPOINT_HEADPHONES, "eq_enabled")
                .await?
                .bool_value
                .unwrap_or(false),
            ..ThxAudioSnapshot::default()
        };

        for (i, slot) in snap.eq_bands.iter_mut().enumerate() {
            let prop_id = format!("eq_band_{}_gain_db", i + 1);
            *slot = self
                .get_property(THX_ENDPOINT_HEADPHONES, &prop_id)
                .await?
                .float_value
                .unwrap_or(0.0);
        }

        snap.spatial_enabled = self
            .get_property(THX_ENDPOINT_HEADPHONES, "spatial_enabled")
            .await?
            .bool_value
            .unwrap_or(false);
        snap.upmix_enabled = self
            .get_property(THX_ENDPOINT_HEADPHONES, "upmix_enabled")
            .await?
            .bool_value
            .unwrap_or(false);
        snap.width_expansion_enabled = self
            .get_property(THX_ENDPOINT_HEADPHONES, "width_expansion_enabled")
            .await?
            .bool_value
            .unwrap_or(false);
        snap.bassboost_enabled = self
            .get_property(THX_ENDPOINT_HEADPHONES, "bassboost_enabled")
            .await?
            .bool_value
            .unwrap_or(false);
        snap.bassboost_level = self
            .get_property(THX_ENDPOINT_HEADPHONES, "bassboost_level")
            .await?
            .float_value
            .unwrap_or(0.55);
        snap.voice_clarity_enabled = self
            .get_property(THX_ENDPOINT_HEADPHONES, "voice_clarity_enabled")
            .await?
            .bool_value
            .unwrap_or(false);
        snap.voice_clarity_level = self
            .get_property(THX_ENDPOINT_HEADPHONES, "voice_clarity_level")
            .await?
            .float_value
            .unwrap_or(0.0);
        snap.volume_leveling_enabled = self
            .get_property(THX_ENDPOINT_HEADPHONES, "volume_leveling_enabled")
            .await?
            .bool_value
            .unwrap_or(false);
        snap.volume_leveling_level = self
            .get_property(THX_ENDPOINT_HEADPHONES, "volume_leveling_level")
            .await?
            .float_value
            .unwrap_or(0.0);

        Ok(snap)
    }

    pub async fn apply_snapshot(&mut self, snap: &ThxAudioSnapshot) -> Result<(), String> {
        let props = snapshot_to_properties(snap);
        for prop in props {
            self.set_property(THX_ENDPOINT_HEADPHONES, prop).await?;
        }
        Ok(())
    }
}

fn make_bool_prop(id: &str, val: bool) -> ThxProperty {
    ThxProperty {
        id: id.to_string(),
        r#type: PROPERTY_TYPE_BOOL,
        float_value: None,
        int_value: None,
        bool_value: Some(val),
        string_value: None,
        string_array_value: Vec::new(),
        float_array_value: Vec::new(),
    }
}

fn make_float_prop(id: &str, val: f32) -> ThxProperty {
    ThxProperty {
        id: id.to_string(),
        r#type: PROPERTY_TYPE_FLOAT,
        float_value: Some(val),
        int_value: None,
        bool_value: None,
        string_value: None,
        string_array_value: Vec::new(),
        float_array_value: Vec::new(),
    }
}

pub fn snapshot_to_properties(snap: &ThxAudioSnapshot) -> Vec<ThxProperty> {
    let mut props = Vec::with_capacity(19);
    props.push(make_bool_prop("eq_enabled", snap.eq_enabled));
    for (i, &gain) in snap.eq_bands.iter().enumerate() {
        props.push(make_float_prop(
            &format!("eq_band_{}_gain_db", i + 1),
            gain.clamp(-12.0, 12.0),
        ));
    }
    props.push(make_bool_prop("spatial_enabled", snap.spatial_enabled));
    props.push(make_bool_prop("upmix_enabled", snap.upmix_enabled));
    props.push(make_bool_prop(
        "width_expansion_enabled",
        snap.width_expansion_enabled,
    ));
    props.push(make_bool_prop("bassboost_enabled", snap.bassboost_enabled));
    props.push(make_float_prop(
        "bassboost_level",
        snap.bassboost_level.clamp(0.1, 1.0),
    ));
    props.push(make_bool_prop(
        "voice_clarity_enabled",
        snap.voice_clarity_enabled,
    ));
    props.push(make_float_prop(
        "voice_clarity_level",
        snap.voice_clarity_level.clamp(0.0, 1.0),
    ));
    props.push(make_bool_prop(
        "volume_leveling_enabled",
        snap.volume_leveling_enabled,
    ));
    props.push(make_float_prop(
        "volume_leveling_level",
        snap.volume_leveling_level.clamp(0.0, 1.0),
    ));
    props
}

/// Converts LumenHID `AudioConfig` JSON into a clamped `ThxAudioSnapshot` targeting `headphones`.
pub fn ui_audio_config_to_snapshot(
    config: &serde_json::Value,
    baseline: Option<&ThxAudioSnapshot>,
) -> ThxAudioSnapshot {
    let mut snap = baseline.cloned().unwrap_or_default();

    let mut eq_bands = [0.0f32; 10];
    let mut any_eq = false;
    if let Some(arr) = config.get("bands").and_then(|v| v.as_array()) {
        for (i, val) in arr.iter().take(10).enumerate() {
            let db = (val.as_f64().unwrap_or(0.0) as f32).clamp(-12.0, 12.0);
            if db.abs() > 0.01 {
                any_eq = true;
            }
            eq_bands[i] = db;
        }
    }
    let active_preset = config
        .get("activePresetId")
        .and_then(|v| v.as_str())
        .unwrap_or("custom");
    snap.eq_bands = eq_bands;
    snap.eq_enabled = any_eq || active_preset != "flat";

    let surround71 = config
        .get("surround71Enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let soundstage_width = config
        .get("soundstageWidth")
        .and_then(|v| v.as_f64())
        .unwrap_or(50.0);
    snap.spatial_enabled = surround71;
    snap.upmix_enabled = surround71;
    snap.width_expansion_enabled = surround71 && soundstage_width >= 50.0;

    let bass_boost = config
        .get("bassBoost")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0)
        .clamp(0.0, 100.0);
    snap.bassboost_enabled = bass_boost > 0.0;
    snap.bassboost_level = if bass_boost > 0.0 {
        ((bass_boost / 100.0) as f32).clamp(0.1, 1.0)
    } else {
        0.55
    };

    let voice_clarity = config
        .get("voiceClarity")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0)
        .clamp(0.0, 100.0);
    snap.voice_clarity_enabled = voice_clarity > 0.0;
    snap.voice_clarity_level = ((voice_clarity / 100.0) as f32).clamp(0.0, 1.0);

    snap
}

pub async fn get_status() -> ThxStatus {
    let cfg = match load_thx_discovery_config() {
        Ok(c) => c,
        Err(err) => {
            return ThxStatus {
                available: false,
                version: None,
                endpoint: None,
                detail: err,
            }
        }
    };
    let endpoint_addr = format!("{}:{}", cfg.host, cfg.port);
    match ThxClient::connect_with_config(&cfg).await {
        Ok(mut client) => match client.get_version().await {
            Ok(version) => {
                let endpoints = client.get_endpoints("").await.unwrap_or_default();
                let has_hp = endpoints.iter().any(|e| e == THX_ENDPOINT_HEADPHONES);
                ThxStatus {
                    available: true,
                    version: Some(version.clone()),
                    endpoint: Some(endpoint_addr),
                    detail: if has_hp {
                        format!("已直连 THX gRPC 服务 v{version}（headphones 端点就绪）")
                    } else {
                        format!("已直连 THX gRPC 服务 v{version}")
                    },
                }
            }
            Err(e) => ThxStatus {
                available: false,
                version: None,
                endpoint: Some(endpoint_addr),
                detail: e,
            },
        },
        Err(e) => ThxStatus {
            available: false,
            version: None,
            endpoint: Some(endpoint_addr),
            detail: e,
        },
    }
}

pub async fn capture_headphones_snapshot() -> Result<ThxAudioSnapshot, String> {
    let mut client = ThxClient::connect().await?;
    client.capture_headphones_snapshot().await
}

pub async fn apply_ui_audio_config(
    config: &serde_json::Value,
    baseline: Option<&ThxAudioSnapshot>,
) -> Result<ThxAudioSnapshot, String> {
    let target = ui_audio_config_to_snapshot(config, baseline);
    let mut client = ThxClient::connect().await?;
    client.apply_snapshot(&target).await?;
    Ok(target)
}

pub async fn restore_thx_snapshot(snapshot: &ThxAudioSnapshot) -> Result<(), String> {
    let mut client = ThxClient::connect().await?;
    client.apply_snapshot(snapshot).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prost_property_round_trip() {
        let req = SetPropertyRequest {
            endpoint_id: Some("headphones".to_string()),
            property: Some(ThxProperty {
                id: "eq_band_3_gain_db".to_string(),
                r#type: PROPERTY_TYPE_FLOAT,
                float_value: Some(4.5),
                int_value: None,
                bool_value: None,
                string_value: None,
                string_array_value: vec![],
                float_array_value: vec![],
            }),
        };

        let framed = pack_grpc_frame(&req).unwrap();
        assert_eq!(framed[0], 0);
        let raw_proto = unpack_grpc_frame(&framed).unwrap();
        let decoded = SetPropertyRequest::decode(raw_proto).unwrap();
        assert_eq!(decoded, req);
    }

    #[test]
    fn grpc_framing_rejects_short_or_compressed_frames() {
        assert!(unpack_grpc_frame(&[0, 0, 0, 1]).is_err());
        assert!(unpack_grpc_frame(&[1, 0, 0, 0, 0]).is_err());
        assert!(unpack_grpc_frame(&[0, 0, 0, 0, 5, 1, 2]).is_err());
    }

    #[test]
    fn discovery_json_validates_loopback_and_decodes_pem() {
        let sample_b64 = BASE64_STANDARD.encode(b"\x30\x82\x01\x01dummy-der-cert");
        let json = serde_json::json!({
            "address": "localhost:49669",
            "cert": format!("-----BEGIN CERTIFICATE-----\n{sample_b64}\n-----END CERTIFICATE-----\n")
        })
        .to_string();

        let cfg = parse_discovery_json(&json).unwrap();
        assert_eq!(cfg.host, "localhost");
        assert_eq!(cfg.port, 49669);
        assert_eq!(cfg.cert_der, b"\x30\x82\x01\x01dummy-der-cert");

        let bad_host = serde_json::json!({
            "address": "example.com:49669",
            "cert": format!("-----BEGIN CERTIFICATE-----\n{sample_b64}\n-----END CERTIFICATE-----\n")
        })
        .to_string();
        assert!(parse_discovery_json(&bad_host).is_err());
    }

    #[test]
    fn maps_ui_audio_config_with_safe_clamping() {
        let ui = serde_json::json!({
            "activePresetId": "fps",
            "bands": [-18.0, 2.5, 4.0, 15.0, 0.0, 1.0, 2.0, 3.0, 4.0, 5.0],
            "surround71Enabled": true,
            "soundstageWidth": 72,
            "bassBoost": 85,
            "voiceClarity": 40
        });
        let snap = ui_audio_config_to_snapshot(&ui, None);
        assert!(snap.eq_enabled);
        assert_eq!(snap.eq_bands[0], -12.0);
        assert_eq!(snap.eq_bands[1], 2.5);
        assert_eq!(snap.eq_bands[3], 12.0);
        assert!(snap.spatial_enabled);
        assert!(snap.upmix_enabled);
        assert!(snap.width_expansion_enabled);
        assert!(snap.bassboost_enabled);
        assert!((snap.bassboost_level - 0.85).abs() < 0.001);
        assert!(snap.voice_clarity_enabled);
        assert!((snap.voice_clarity_level - 0.40).abs() < 0.001);

        let zero_bass = serde_json::json!({
            "activePresetId": "flat",
            "bands": [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            "surround71Enabled": false,
            "soundstageWidth": 20,
            "bassBoost": 0,
            "voiceClarity": 0
        });
        let snap_flat = ui_audio_config_to_snapshot(&zero_bass, None);
        assert!(!snap_flat.eq_enabled);
        assert!(!snap_flat.bassboost_enabled);
        assert!((snap_flat.bassboost_level - 0.55).abs() < 0.001);
        assert!(!snap_flat.voice_clarity_enabled);
        assert!(!snap_flat.spatial_enabled);
    }

    fn der_tlv(tag: u8, content: &[u8]) -> Vec<u8> {
        let mut out = vec![tag];
        if content.len() < 128 {
            out.push(content.len() as u8);
        } else {
            let bytes = content.len().to_be_bytes();
            let first_non_zero = bytes.iter().position(|&b| b != 0).unwrap_or(bytes.len() - 1);
            let tail = &bytes[first_non_zero..];
            out.push(0x80 | (tail.len() as u8));
            out.extend_from_slice(tail);
        }
        out.extend_from_slice(content);
        out
    }

    #[test]
    fn extracts_spki_from_x509_v1_and_v3_der() {
        let rsa_alg_inner = [
            0x06, 0x09, 0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x01, 0x05, 0x00,
        ];
        let fake_pk_payload = [0x30, 0x05, 0x02, 0x01, 0x03, 0x02, 0x01];
        let mut bit_string = vec![0x00u8];
        bit_string.extend_from_slice(&fake_pk_payload);

        let mut spki_body = der_tlv(0x30, &rsa_alg_inner);
        spki_body.extend_from_slice(&der_tlv(0x03, &bit_string));
        let spki = der_tlv(0x30, &spki_body);

        let mut common_fields = Vec::new();
        common_fields.extend_from_slice(&der_tlv(0x02, &[0x01]));
        common_fields.extend_from_slice(&der_tlv(0x30, &rsa_alg_inner));
        common_fields.extend_from_slice(&der_tlv(0x30, &[]));
        common_fields.extend_from_slice(&der_tlv(0x30, &[]));
        common_fields.extend_from_slice(&der_tlv(0x30, &[]));
        common_fields.extend_from_slice(&spki);

        // X.509 v1 (no [0] version field)
        let tbs_v1 = der_tlv(0x30, &common_fields);
        let cert_v1 = der_tlv(0x30, &tbs_v1);
        let (alg_v1, pk_v1) = extract_spki_from_x509_der(&cert_v1).unwrap();
        assert_eq!(alg_v1, rsa_alg_inner);
        assert_eq!(pk_v1, fake_pk_payload);

        // X.509 v3 (explicit [0] INTEGER 2 prefix + trailing [3] extensions)
        let mut v3_fields = der_tlv(0xA0, &der_tlv(0x02, &[0x02]));
        v3_fields.extend_from_slice(&common_fields);
        v3_fields.extend_from_slice(&der_tlv(0xA3, &[]));
        let tbs_v3 = der_tlv(0x30, &v3_fields);
        let cert_v3 = der_tlv(0x30, &tbs_v3);
        let (alg_v3, pk_v3) = extract_spki_from_x509_der(&cert_v3).unwrap();
        assert_eq!(alg_v3, rsa_alg_inner);
        assert_eq!(pk_v3, fake_pk_payload);
    }

    /// Strictly read-only live verification of the local HECATE G5 Max THX gRPC service.
    /// Zero `SetProperty` mutations are performed.
    #[test]
    #[ignore = "Read-only live THX gRPC verification"]
    fn live_readonly_thx_grpc_check() {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("tokio runtime");
        rt.block_on(async {
            let status = get_status().await;
            println!("LIVE THX STATUS => {:?}", status);
            assert!(status.available, "Expected local THX service to be reachable: {}", status.detail);
            let snap = capture_headphones_snapshot()
                .await
                .expect("capture_headphones_snapshot should succeed in read-only mode");
            println!("LIVE THX HEADPHONES SNAPSHOT => {:?}", snap);
        });
    }
}
