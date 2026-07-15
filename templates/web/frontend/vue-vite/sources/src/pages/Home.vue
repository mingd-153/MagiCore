<template>
  <div>
    <h1>{{ title }}</h1>
    <p>{{ description }}</p>
  </div>
</template>

<script>
export default {
  name: "HomePage",
  data() {
    return { title: "Home", description: "Welcome to Vue + Vite" };
  },
};
</script>

<style scoped>
h1 { font-size: 2rem; }
</style>