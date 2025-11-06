# Spam-rs

A system for sending mail via the datasektionen AWS SES service.

## API

`[WIP]`

## Legacy

### API

> [!CAUTION]
> This API is deprecated and will be removed in the future.
> Also, `multipart/form-data` is not supported in the legacy API.

#### `POST /api/legacy/sendmail`

Send an email to one or more recipients. Requests can be sent using
JSON. Be sure to set the `Content-Type` header to
`application/json`.

The following fields are required:

- `from`: The email address to send the email from. Must be a verified
  email address, i.e; `@datasektionen.se`, `@metaspexet.se`, `ddagen.se`.
- `to`: A list of email addresses to send the email to.
- `subject`: The subject of the email.

Either `content` or `html` must be provided:

- `content`: The plain text content of the email. This gets rendered
  into HTML using a markdown parser.
- `html`: The HTML content of the email. This will **not** be converted
  from markdown, and will just be sent as HTML.

If both `content` and `html` are provided, `html` will be used.

The following field are optional:

- `replyTo`: The email address to set as the reply-to address.
- `cc`: A list of email addresses to send a copy of the email to.
- `bcc`: A list of email addresses to send a blind copy of the email
  to.
- `template`: The name of a template to use for the email. There are
  currently three templates available:
  - `default`: A simple template with a header and footer in the
    Datasektionen style. This is the default if **no template** is
    specified.
  - `metaspexet`: A similar template but in the style of metaspexet.
  - `none`: A raw template with no styling. Use this if you want to
    provide your own HTML.

- `attachments[]`: Attachments to include in the email. A maximum of 5
  files can be attached. An attachment sent needs the JSON object to include the `originalname`,
  `buffer` (the file contents), and `mimetype`. You can also
  supply the `encoding` parameter, i.e; `base64` or `utf-8`. If no
  encoding is provided, `base64` will be used.

An example of a valid JSON request:

```json
{
  "key": "your key",
  "template": "default",
  "from": {
    "name": "Ture Teknolog",
    "address": "turetek@datasektionen.se"
  },
  "replyTo": "noreply@datasektionen.se",
  "to": ["recipient@domain.org", "another@one.com"],
  "subject": "Hello, world!",
  "content": "This is the plain text content of the email.",
  "cc": [
    {
      "name": "Herr/Fru Ordförande",
      "address": "ordf@datasektionen.se"
    }
  ],
  "bcc": ["very@secret.com"],
  "attachments[]": [
    {
      "originalname": "file.txt",
      "buffer": "Hello World!",
      "mimetype": "text/plain",
      "encoding": "utf8"
    }
  ]
}
```

#### `GET /api/legacy/ping`

Returns "I'm alive!" if the server is running.

# Spam

![spam](http://media.boingboing.net/wp-content/uploads/2016/01/Spam-Can.jpg)
