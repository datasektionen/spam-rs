job "spam" {
  type = "service"

  group "spam" {
    count = 2

    network {
      port "http" { }
    }

    service {
      name     = "spam"
      port     = "http"
      provider = "nomad"
      tags = [
        "traefik.enable=true",
        "traefik.http.routers.spam.rule=Host(`spam.datasektionen.se`)",
        "traefik.http.routers.spam.tls.certresolver=default",
      ]

      check {
        type     = "http"
        path     = "/api/ping"
        interval = "10s"
        timeout  = "2s"
      }
    }

    task "spam" {
      driver = "docker"

      config {
        image = var.image_tag
        ports = ["http"]
      }

      template {
        data        = <<ENV
{{ with nomadVar "nomad/jobs/spam" }}
APP_SECRET={{ .app_secret }}
HIVE_SECRET={{ .hive_secret }}
AWS_ACCESS_KEY_ID={{ .aws_key_id }}
AWS_SECRET_ACCESS_KEY={{ .aws_key_secret }}
{{ end }}
PORT={{ env "NOMAD_PORT_http" }}
HIVE_URL=https://hive.datasektionen.se/api/v1
HOST_ADDRESS=0.0.0.0
RUST_LOG=info
AWS_REGION=eu-west-1
ENV
        destination = "local/.env"
        env         = true
      }

      resources {
        memory = 50
        cpu = 60
      }
    }
  }
}

variable "image_tag" {
  type = string
  default = "ghcr.io/datasektionen/spam-rs:latest"
}
