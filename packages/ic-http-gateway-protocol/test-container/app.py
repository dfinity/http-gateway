'''
a simple http server that returns 429s for all post requests
'''
from flask import Flask, request, jsonify

app = Flask(__name__)

@app.route('/healthcheck', methods=['GET'])
def healthcheck():
    return "ok"

@app.route('/<path:any_path>', methods=['POST'])
def handle_post(any_path):
    return "You're making too many requests", 429

if __name__ == '__main__':
    app.run('0.0.0.0', port=8000)

