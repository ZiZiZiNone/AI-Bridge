<template>
  <ConverterPanel
    :converters="converters.converters.value"
    :state="converters.state.value"
    :error-message="converters.errorMessage.value"
    @retry="converters.reload"
  />
</template>

<script setup lang="ts">
import ConverterPanel from '../panels/ConverterPanel.vue';
import { useConverters } from '../../hooks/useConverters';

/**
 * W8 转换器容器（Page 层）：持有转换器清单 hook。
 */
const converters = useConverters();

void converters.reload();
</script>
