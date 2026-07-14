#[cfg(test)]
mod hasher_tests {
    use crate::hasher::hash_content;

    #[test]
    fn hash_content_deterministic() {
        let hash1 = hash_content("hello world");
        let hash2 = hash_content("hello world");
        assert_eq!(hash1, hash2);
    }

    #[test]
    fn hash_content_different_inputs_different_hashes() {
        let hash1 = hash_content("hello");
        let hash2 = hash_content("world");
        assert_ne!(hash1, hash2);
    }

    #[test]
    fn hash_content_is_sha256_hex() {
        let hash = hash_content("test");
        assert_eq!(hash.len(), 64); // SHA-256 = 32 bytes = 64 hex chars
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn hash_content_empty_string() {
        let hash = hash_content("");
        // SHA-256 of empty string is well-known
        assert_eq!(hash, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
    }

    #[test]
    fn hash_content_unicode() {
        let hash1 = hash_content("你好世界");
        let hash2 = hash_content("你好世界");
        assert_eq!(hash1, hash2);
        assert_eq!(hash1.len(), 64);
    }

    #[tokio::test]
    async fn hash_file_nonexistent_returns_error() {
        let path = std::path::Path::new("/nonexistent/file.txt");
        let result = crate::hasher::hash_file(path).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn hash_file_works_on_real_file() {
        let dir = std::env::temp_dir().join("nova_test_hash");
        std::fs::create_dir_all(&dir).unwrap();
        let file_path = dir.join("test.txt");
        std::fs::write(&file_path, b"test content").unwrap();

        let hash = crate::hasher::hash_file(&file_path).await.unwrap();
        assert_eq!(hash.len(), 64);

        // hash_file should match hash_content for the same bytes
        let content_hash = hash_content("test content");
        assert_eq!(hash, content_hash);

        // Cleanup
        std::fs::remove_dir_all(&dir).ok();
    }
}

#[cfg(test)]
mod cover_helpers_tests {
    use crate::cover;

    #[test]
    fn detect_jpeg() {
        let data: Vec<u8> = vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10];
        let format = cover::detect_image_format_pub(&data);
        assert_eq!(format, Some("jpg"));
    }

    #[test]
    fn detect_png() {
        let data: Vec<u8> = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A];
        let format = cover::detect_image_format_pub(&data);
        assert_eq!(format, Some("png"));
    }

    #[test]
    fn detect_gif() {
        let data: Vec<u8> = vec![0x47, 0x49, 0x46, 0x38, 0x39, 0x61];
        let format = cover::detect_image_format_pub(&data);
        assert_eq!(format, Some("gif"));
    }

    #[test]
    fn detect_webp() {
        let data: Vec<u8> = vec![0x52, 0x49, 0x46, 0x46, 0x00, 0x00];
        let format = cover::detect_image_format_pub(&data);
        assert_eq!(format, Some("webp"));
    }

    #[test]
    fn detect_unknown_format() {
        let data: Vec<u8> = vec![0x00, 0x01, 0x02, 0x03];
        let format = cover::detect_image_format_pub(&data);
        assert_eq!(format, None);
    }

    #[test]
    fn detect_too_short_data() {
        let data: Vec<u8> = vec![0xFF, 0xD8];
        let format = cover::detect_image_format_pub(&data);
        assert_eq!(format, None);
    }

    #[test]
    fn find_cover_id_in_opf_standard() {
        let opf = r#"<metadata>
            <meta name="cover" content="cover-image"/>
        </metadata>"#;
        let id = cover::find_cover_id_in_opf_pub(opf);
        assert_eq!(id.as_deref(), Some("cover-image"));
    }

    #[test]
    fn find_cover_id_in_opf_single_quotes() {
        let opf = r#"<meta name='cover' content='my-cover'/>"#;
        let id = cover::find_cover_id_in_opf_pub(opf);
        assert_eq!(id.as_deref(), Some("my-cover"));
    }

    #[test]
    fn find_cover_id_in_opf_missing() {
        let opf = r#"<metadata><meta name="title" content="My Book"/></metadata>"#;
        let id = cover::find_cover_id_in_opf_pub(opf);
        assert_eq!(id, None);
    }

    #[test]
    fn find_href_by_id_found() {
        let opf = r#"<manifest>
            <item id="cover-image" href="images/cover.jpg" media-type="image/jpeg"/>
        </manifest>"#;
        let href = cover::find_href_by_id_pub(opf, "cover-image");
        assert_eq!(href.as_deref(), Some("images/cover.jpg"));
    }

    #[test]
    fn find_href_by_id_not_found() {
        let opf = r#"<manifest><item id="other" href="other.html"/></manifest>"#;
        let href = cover::find_href_by_id_pub(opf, "cover-image");
        assert_eq!(href, None);
    }
}
