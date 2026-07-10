import os
import re

mig_dir = 'migrations'
for f in os.listdir(mig_dir):
    if not f.endswith('.sql'): continue
    path = os.path.join(mig_dir, f)
    with open(path, 'r') as file:
        content = file.read()
    
    # Replacements
    content = re.sub(r'UUID PRIMARY KEY DEFAULT gen_random_uuid\(\)', 'TEXT PRIMARY KEY', content)
    content = re.sub(r'UUID PRIMARY KEY', 'TEXT PRIMARY KEY', content)
    content = content.replace('JSONB', 'TEXT')
    content = content.replace('TIMESTAMP WITH TIME ZONE', 'DATETIME')
    content = content.replace('BIGINT', 'INTEGER')
    content = content.replace('gen_random_uuid()', 'lower(hex(randomblob(16)))')
    
    with open(path, 'w') as file:
        file.write(content)
print("Done")
