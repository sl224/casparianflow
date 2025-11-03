from sqlalchemy.orm import Mapped, mapped_column
from casp_sa_base import Base
from sqlalchemy import ForeignKey, func
from datetime import datetime
from typing import Optional


# disable for prod -- for allowing jupyter cells

extend_existing = True

class FolderRecord(Base):
    __tablename__ = "folder"
    folder_id: Mapped[int] = mapped_column(primary_key=True, autoincrement=True)
    folder_path: Mapped[str] = mapped_column(unique=True)

class FileRecord(Base):
    __tablename__ = "file"
    folder_id: Mapped[int] = mapped_column(ForeignKey("folder.folder_id"))
    file_id: Mapped[int] = mapped_column(primary_key=True, autoincrement=True)
    file_name: Mapped[str] 
    filesize_bytes: Mapped[int]
    # inserted_at: Mapped[datetime] = mapped_column(server_default=func.now())

class ProcessingLog(Base):
    __tablename__ = "processing_status"
    process_id: Mapped[int] = mapped_column(primary_key=True, autoincrement=True)
    processing_start: Mapped[datetime] 
    processing_end: Mapped[Optional[datetime]] 
    process_status: Mapped[str] 
    status_updated_at: Mapped[datetime]
    # process_specific_status_table_id: Mapped[int]
