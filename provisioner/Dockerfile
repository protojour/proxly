FROM alpine:latest
RUN apk add iptables
USER root
COPY provision.sh .

ENTRYPOINT ["./provision.sh"]
CMD ["--help"]
