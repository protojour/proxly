# default target
target := "x86_64-unknown-linux-musl"

# build protojour/proxly:dev debug image
dev-image:
    docker build . -t protojour/proxly:dev --platform linux/amd64 --build-arg RUST_PROFILE=debug --build-arg CARGO_FLAGS=

release-image:
    docker build . -t protojour/proxly:dev --platform linux/amd64 --build-arg RUST_PROFILE=release --build-arg CARGO_FLAGS=--release

provisioner-image:
    docker build provisioner -t protojour/proxly-provisioner:dev --platform linux/amd64

testservice-image:
    cross build -p proxly-testservice --target x86_64-unknown-linux-musl --target-dir target-musl
    docker build . -t protojour/proxly-testservice:dev -f testservice.Dockerfile

k8s-demo-deploy: dev-image provisioner-image testservice-image
    # idempotent preparation
    HELM_MAX_HISTORY=2 \
        helm upgrade --install openbao ./testfiles/k8s/charts/openbao-authly-dev-0.0.2.tgz \
        --namespace openbao-authly-dev --create-namespace

    -kubectl create namespace proxly-test
    kubectl create configmap authly-documents \
        -n proxly-test \
        --from-file=testfiles/k8s/authly-documents -o yaml \
        --dry-run=client | kubectl apply -f -

    HELM_MAX_HISTORY=2 \
        helm upgrade --install authly ./testfiles/k8s/charts/authly-0.0.5.tgz \
        --namespace proxly-test \
        -f testfiles/k8s/authly-test-values.yaml

    # (re-)deploy extra things for the demo
    kubectl apply \
        -f testfiles/k8s/ts-proxied.yaml \
        -f testfiles/k8s/ts-unproxied.yaml

    # restart pods
    kubectl delete pods --namespace=proxly-test -l 'proxlyDev=restart' --wait=false
