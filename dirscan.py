#%%
import os
from pathlib import Path

# Set the current working directory to the directory of this script.
# This is useful when running from a different directory (e.g., in a Jupyter cell or IDE).
try:
    os.chdir(os.path.dirname(os.path.abspath(__file__)))
except NameError:
    # This is expected in some interactive environments like Jupyter
    pass

from sqlalchemy import (
    create_engine,
    inspect,
)
from sqlalchemy.orm import DeclarativeBase, Mapped, mapped_column

# 1. Define a declarative base with the new to_dict method
class Base(DeclarativeBase):
    def to_dict(self, exclude_pk=True):
        """
        Return a dictionary representation of the object's mapped columns.
        
        Excludes the internal SQLAlchemy state.
        By default, it also excludes primary key columns.
        """
        mapper = inspect(self.__class__)
        
        dict_rep = {}
        for c in mapper.column_attrs:
            if exclude_pk and c.columns[0].primary_key:
                continue
            dict_rep[c.key] = getattr(self, c.key)
            
        return dict_rep

    @classmethod
    def insert(cls):
        return cls.__table__.insert()

# 2. Define the ORM class, which is the single source of truth for the table schema.
class FileRecord(Base):
    __tablename__ = "file"

    file_id: Mapped[int] = mapped_column(primary_key=True, autoincrement=True)
    file_path: Mapped[str] = mapped_column(unique=True) # Good practice to ensure paths are unique
    filesize_bytes: Mapped[int]

# Using sets for faster lookups
skip_dirs = set()
skip_files = set()

def is_skip_dir(check_dir):
    return check_dir in skip_dirs

def is_skip_file(check_file):
    return check_file in skip_files

def scan(dirname):
    engine = create_engine("sqlite:///./test.db")
    Base.metadata.create_all(engine)

    file_records = []
    for cur_dir, dirs, files in os.walk(dirname):
        # Prune directories in-place to prevent os.walk from descending into them
        dirs[:] = [d for d in dirs if not is_skip_dir(os.path.join(cur_dir, d))]

        for file in files:
            if is_skip_file(file): continue
            cur_path = Path(cur_dir) / file
            try:
                stat_result = cur_path.stat()
                file_records.append(FileRecord(file_path=str(cur_path), filesize_bytes=stat_result.st_size))
            except FileNotFoundError:
                print(f"Warning: {cur_path} was listed but not found. Skipping.")

    if not file_records:
        print("No files found to insert.")
        return

    # 3. Use the new to_dict() method to prepare data for the high-speed insert.
    rows_to_insert = [rec.to_dict() for rec in file_records]

    with engine.begin() as conn:
        result = conn.execute(FileRecord.insert(), rows_to_insert)
        print(f"Successfully inserted {result.rowcount} rows into test.db.")

if __name__ == '__main__':
    db_path = 'test.db'
    if os.path.exists(db_path):
        os.remove(db_path)
        print(f"Successfully deleted existing db: {db_path}")
    scan('.')

# %%
