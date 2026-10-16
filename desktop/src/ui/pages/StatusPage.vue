<template>
  <StatusPanel
    :sessions="matrix.rows.value"
    :state="matrix.state.value"
    :error-message="matrix.errorMessage.value"
    @retry="matrix.reload"
  />
</template>

<script setup lang="ts">
import StatusPanel from '../panels/StatusPanel.vue';
import { useSessionMatrix } from '../../hooks/useSessionMatrix';

/**
 * W4 状态清单容器（Page 层）：持有会话矩阵 hook，把数据下传、把 retry 接住。
 */
const matrix = useSessionMatrix();

void matrix.reload();
</script>
