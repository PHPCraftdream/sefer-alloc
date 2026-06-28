# NUMA release gate — cloud multi-socket VM recipe

Phase 4 deliverable for the NUMA testing strategy (task #99 / Phase 4 of
[`NUMA_TESTING_OPTIONS.md`](NUMA_TESTING_OPTIONS.md)).

> **When to run.** Before any release tagged `0.x.y` whose diff touches
> `crates/numa/**`, `src/alloc_core/numa.rs`, or
> `src/alloc_core/segment_header.rs::node_id`. **Skip** for patch releases
> that don't touch those files — Phase 1 (mock) + Phase 2 (real Linux
> kernel) + Phase 3 (Hyper-V virtual NUMA) already cover the day-to-day
> code paths.

## Why we need this

Phases 1-3 validate that our code:
- Calls the NUMA syscalls with the right arguments (Phase 1 mock)
- Compiles + runs on a real Linux kernel (Phase 2 single-NUMA + QEMU)
- Compiles + runs on a real Windows kernel under virtual NUMA topology
  (Phase 3 Hyper-V)

What they **don't** validate:
- **Real multi-socket page-placement.** A single-socket host always
  serves physical memory from one node, no matter what NUMA syscall
  was issued. The kernel's hint is honored only when there are *two
  physical nodes* to choose from.
- **Real cross-socket latency.** Cross-socket memory access on real
  silicon is 1.5-3× slower than local-node access. None of our synthetic
  setups can simulate this.
- **BIOS / firmware topology quirks.** Some 2-socket boxes expose 2
  NUMA nodes; some expose 4 (sub-NUMA clustering); some expose 1 (BIOS
  setting). Real hardware finds bugs synthetic NUMA cannot.

A 10-minute spot-instance run on real multi-socket silicon catches the
class of bug that only appears at scale — and it costs less than $5.

## Recipe (AWS, primary)

Pick a metal instance with ≥2 sockets. As of mid-2026:

| Instance | Sockets | RAM | Spot $/hr (us-east-1, varies) |
|----------|---------|-----|--------------------------------|
| `c5n.metal` | 2 | 192 GiB | ~$1.50 |
| `m5n.metal` | 2 | 384 GiB | ~$2.00 |
| `r5n.metal` | 2 | 768 GiB | ~$2.50 |

`c5n.metal` is the cheapest 2-socket option; use that unless you need
more RAM for stress testing.

### Launch

```bash
# Prereqs: AWS CLI configured, key pair created.
# (Replace placeholders with your values.)
INSTANCE_TYPE=c5n.metal
AMI=ami-053b0d53c279acc90    # Amazon Linux 2023 in us-east-1 (verify current)
KEY=<your-key-name>
SG=<your-default-sg>          # must allow inbound SSH from your IP
SUBNET=<your-public-subnet>

aws ec2 run-instances \
  --instance-type "$INSTANCE_TYPE" \
  --image-id "$AMI" \
  --key-name "$KEY" \
  --security-group-ids "$SG" \
  --subnet-id "$SUBNET" \
  --instance-market-options 'MarketType=spot' \
  --tag-specifications \
     'ResourceType=instance,Tags=[{Key=Name,Value=sefer-alloc-numa-gate}]' \
  --query 'Instances[0].InstanceId' \
  --output text
```

Wait for the instance to enter `running` state (1-2 minutes), grab its
public IP:

```bash
INSTANCE_ID=<id-from-above>
PUBLIC_IP=$(aws ec2 describe-instances \
    --instance-ids "$INSTANCE_ID" \
    --query 'Reservations[0].Instances[0].PublicIpAddress' \
    --output text)
echo "Connect: ssh ec2-user@$PUBLIC_IP"
```

### Sanity-check the host

```bash
ssh ec2-user@$PUBLIC_IP
sudo dnf install -y numactl gcc

# MUST show 2 nodes. If it shows 1, you got the wrong instance type.
numactl --hardware
# Look for:
#   available: 2 nodes (0-1)
#   node 0 cpus: 0 1 ... <half of cores>
#   node 1 cpus: ... <other half>
```

If `available: 1 nodes`, **stop here** — wrong instance type, terminate
and pick a real 2-socket type. Continuing would defeat the purpose.

### Install Rust + run the suite

```bash
# Rustup default toolchain (stable).
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y --default-toolchain stable
source $HOME/.cargo/env

# Clone the release commit (replace TAG):
git clone https://github.com/PHPCraftdream/sefer-alloc.git
cd sefer-alloc
git checkout v0.x.y   # the release tag being gated

# Build with full feature set + numa-aware:
cargo build --release --features "production numa-aware"

# Run the full env-guarded NUMA suite — these tests actively use the
# 2-node topology (cross-thread free between segments on different nodes,
# mbind followed by page-fault to verify physical placement, etc.):
export SEFER_NUMA_TEST=1
cargo test --release --features "production numa-aware" --test numa_alloc -- --nocapture
cargo test --release --features "production numa-aware" --test numa_segment_id
cargo test --release --features "production numa-aware" --test numa_seam

# Verify physical placement of NUMA-bound allocations.  /proc/self/numa_maps
# shows the node each VAD region is backed by; we grep our test binary's
# segments and assert they match the requested node.
cat /proc/self/numa_maps | grep -E "huge|N0|N1" | head -20

# Optional: stress test with both nodes.
numactl --interleave=0,1 cargo test --release --features "production numa-aware"
```

### Destroy the instance

**Do this immediately when done — spot instances bill by the second
but a forgotten `c5n.metal` costs $36/day.**

```bash
aws ec2 terminate-instances --instance-ids "$INSTANCE_ID"
```

## Recipe (Azure, alternative)

Equivalent metal instance: `HBv4`-series (2 sockets, AMD EPYC). Workflow
is the same; replace the AWS CLI with Azure CLI:

```bash
az vm create \
  --resource-group sefer-alloc-test \
  --name numa-gate \
  --image Ubuntu2404 \
  --size Standard_HB176-96rs_v4 \
  --priority Spot \
  --max-price -1 \
  --eviction-policy Delete \
  --generate-ssh-keys
# ... same numactl + cargo test as AWS ...
az vm delete --resource-group sefer-alloc-test --name numa-gate --yes
```

Azure spot pricing is comparable (~$1.50-$3/hr depending on region).

## Recipe (GCP, NOT recommended for 2-socket)

GCP's general-purpose families (`c2`, `c3`, `n2`) typically expose
**single-NUMA** even on 2-socket boxes — the platform abstracts away the
topology. For 2-socket validation prefer AWS or Azure. (GCP's HPC
families like `h3` are single-socket. For NUMA gating GCP is the
wrong cloud.)

## Expected output

```
test result: ok. <N> passed; 0 failed; 0 ignored; ...
```

If anything fails:
1. **Capture the full output** including `numactl --hardware` and
   `/proc/self/numa_maps` excerpts.
2. **Do NOT release** the version being gated.
3. File an issue on the repo with the failure and the gate-machine
   topology. Roll the fix, re-run the gate.

## Gate budget

Target: < $5 per release-gate run, < 30 minutes wall-clock end-to-end
including instance provisioning. Real numbers from a `c5n.metal` spot
in us-east-1:

- Provision: 90 seconds (instance state `pending` → `running`)
- Build (release, all features): 3-4 minutes (~256 vCPUs but cargo
  parallelism caps out earlier)
- Test suite (~6 NUMA tests under env): 30 seconds
- Total: ~6 minutes runtime + ~5 minutes margin for manual SSH steps
- Cost at $1.50/hr spot: ~$0.15

The 30-minute target is dominated by human interactive time, not
compute. A scripted version (single bash script doing the whole flow)
would cut to ~$0.15 / 8 minutes.

## Who runs this

The release-cutting maintainer. Requires:
- Personal AWS/Azure account with billing enabled
- Trust to run `terminate-instances` correctly
- ~30 minutes of attention

If no maintainer can run it for a given release, the release notes must
explicitly say "NUMA gate skipped this release — code paths in
`crates/numa/` and `src/alloc_core/numa.rs` were not touched, so per the
gating policy this is acceptable". Don't release while saying "we will
gate it later".

## Related

- [`NUMA_TESTING_OPTIONS.md`](NUMA_TESTING_OPTIONS.md) — the master Phase 1-4 plan
- [`NUMA_WINDOWS_DEV_RECIPE.md`](NUMA_WINDOWS_DEV_RECIPE.md) — Phase 3 Hyper-V dev gate
- [`PHASE_NUMA_DESIGN.md`](PHASE_NUMA_DESIGN.md) — design of the NUMA-aware path itself
