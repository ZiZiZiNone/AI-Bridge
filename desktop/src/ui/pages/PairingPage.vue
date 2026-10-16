<template>
  <PairingPanel
    :offer="pairing.offer.value"
    :error-message="pairing.errorMessage.value"
    :action-message="pairing.actionMessage.value"
    :peer-fingerprint="pairing.peerFingerprint.value"
    :is-busy="pairing.isBusy.value"
    @retry="pairing.reload"
    @confirm="pairing.confirm"
    @reject="pairing.reject"
  />
</template>

<script setup lang="ts">
import PairingPanel from '../panels/PairingPanel.vue';
import { usePairing } from '../../hooks/usePairing';

/**
 * W9 配对容器（Page 层）：持有配对 hook；确认 / 拒绝意图直达 hook。
 */
const pairing = usePairing();

void pairing.reload();
</script>
