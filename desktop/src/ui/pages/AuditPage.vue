<template>
  <AuditPanel
    :events="trail.events.value"
    :state="trail.state.value"
    :error-message="trail.errorMessage.value"
    @retry="trail.reload"
    @query="trail.query"
  />
</template>

<script setup lang="ts">
import AuditPanel from '../panels/AuditPanel.vue';
import { useAuditTrail } from '../../hooks/useAuditTrail';

/**
 * W6 审计容器（Page 层）：持有审计流 hook；筛选意图直达 query，retry 沿用最近条件。
 */
const trail = useAuditTrail();

void trail.reload();
</script>
