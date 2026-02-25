---
description: Explain what the provided code does
inputs:
  - name: question
    prompt: "Specific question (optional): "
---
{% if stdin %}
{{ stdin }}

{% endif %}
Explain what this code does. Be concise and focus on the key logic.{% if inputs.question %} In particular: {{ inputs.question }}.{% endif %}
