#[cfg(test)]
mod parsing_tester {
    use crate::smtp::mail::*;

    #[test]
    fn test_parse_body_multipart() {
        let body = std::fs::read_to_string("test/samples/discord_mail.body").unwrap();
        let subject = get_subject(&body);
        println!("subject: {:?}", subject);
        let mail = Mail {
            from: Default::default(),
            to: Default::default(),
            data: body,
            subject,
            id: 0,
        };

        let parsed = mail.parse_body();
        assert!(parsed.starts_with("<!doctype html>"));

        let (from, _) = get_data_from_to(&mail.data);
        assert!(from.contains("noreply@discord.com"));

        //should've decoded the subject with rfc2047 decoder
        assert_eq!(mail.subject.unwrap(), "Vérifie ton adresse e-mail Discord");
    }

    #[test]
    fn test_parse_body_simple() {
        let body = std::fs::read_to_string("test/samples/raw.body").unwrap();
        let subject = get_subject(&body);
        let mail = Mail {
            from: Default::default(),
            to: Default::default(),
            data: body,
            subject,
            id: 0,
        };

        let parsed = mail.parse_body();
        assert_eq!(parsed.len(), 1809);

        let (from, to) = get_data_from_to(&mail.data);
        assert!(from.contains("test@test.com"));
        assert_eq!(to.len(), 8);

        assert_eq!(mail.subject.unwrap(), "test smtp--");
    }
}
