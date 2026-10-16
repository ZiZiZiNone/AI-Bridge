<template>
  <TemporarySessionPanel
    :sessions="sessions.sessions.value"
    :state="sessions.state.value"
    :error-message="sessions.errorMessage.value"
    :action-message="sessions.actionMessage.value"
    :form-visible="formVisible"
    :is-busy="sessions.isBusy.value"
    @retry="sessions.reload"
    @open-form="formVisible = true"
    @close-form="formVisible = false"
    @issue="handleIssue"
    @revoke-all="sessions.revokeAll"
  />
</template>

<script setup lang="ts">
import { ref } from 'vue';
import TemporarySessionPanel from '../panels/TemporarySessionPanel.vue';
import { useTemporarySessions } from '../../hooks/useTemporarySessions';
import type { IssueParams } from '../../types/commands';

/**
 * W5 临时会话容器（Page 层）：持有会话总览 hook 与表单可见性。
 */
const sessions = useTemporarySessions();
const formVisible = ref(false);

void sessions.reload();

async function handleIssue(params: IssueParams): Promise<void> {
  if (await sessions.issue(params)) formVisible.value = false;
}
</script>
