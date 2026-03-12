FROM python:3.13.0-bookworm

# Prevents Python from buffering stdout and stderr
ENV PYTHONUNBUFFERED=1

# Set the working directory in the container
WORKDIR /app

# Move everything into the container
COPY . .

# Install dependencies
RUN pip install -r requirements.txt

# expose the server port
EXPOSE 8000

# a health check so we can wait for testcontainers
HEALTHCHECK --interval=1s --timeout=1s --start-period=1s --retries=3 \
  CMD curl --fail http://localhost:8000/healthcheck || exit 1

# Run the flask server
CMD ["python", "app.py"]
