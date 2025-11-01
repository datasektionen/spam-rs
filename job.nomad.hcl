job "spam-rs" {
  type = "service"

  group "spam-rs" {
    network {
      port "http" { }
    }

    service {
      name     = "spam-rs"
      port     = "http"
      provider = "nomad"
      tags = [
        "traefik.enable=true",
        "traefik.http.routers.spam-rs.rule=Host(`spam.betasektionen.se`)",
        "traefik.http.routers.spam-rs.tls.certresolver=default",
      ]
    }

    task "spam-rs" {
      driver = "docker"

      config {
        image = var.image_tag
        ports = ["http"]
      }

      template {
        data        = <<ENV
{{ with nomadVar "nomad/jobs/spam-rs" }}
APP_SECRET={{ .app_secret }}
HIVE_SECRET={{ .hive_secret }}
AWS_ACCESS_KEY_ID: {{ .aws_key_id }}
AWS_SECRET_ACCESS_KEY: {{ .aws_key_secret }}
{{ end }}
PORT={{ env "NOMAD_PORT_http" }}
HIVE_URL=https://hive.datasektionen.se/api/v1
HOST_ADDRESS=0.0.0.0
RUST_LOG=info
ENV
        destination = "local/.env"
        env         = true
      }

      resources {
        memory = 120
      }
    }
  }
}

variable "image_tag" {
  type = string
  default = "ghcr.io/datasektionen/spam-rs:latest"
}
