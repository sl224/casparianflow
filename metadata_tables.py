from sqlalchemy.orm import Mapped, mapped_column
from casp_sa_base import Base
from sqlalchemy import ForeignKey
from sqlalchemy.sql import func 
from datetime import datetime
from typing import Optional


# disable for prod -- for allowing jupyter cells

extend_existing = True

class FolderRecord(Base):
    __tablename__ = "folder"
    id: Mapped[int] = mapped_column(primary_key=True, autoincrement=True)
    process_id: Mapped[int] = mapped_column(ForeignKey("processing_status.id"))
    folder_path: Mapped[str] = mapped_column(unique=True)
    inserted_at: Mapped[datetime] = mapped_column(server_default=func.now())

class FileRecord(Base):
    __tablename__ = "file"
    id: Mapped[int] = mapped_column(primary_key=True, autoincrement=True)
    process_id: Mapped[int] = mapped_column(ForeignKey("processing_status.id"))
    folder_id: Mapped[int] = mapped_column(ForeignKey("folder.id"))
    file_name: Mapped[str] 
    filesize_bytes: Mapped[int]
    inserted_at: Mapped[datetime] = mapped_column(server_default=func.now())

class ProcessingLog(Base):
    __tablename__ = "processing_status"
    id: Mapped[int] = mapped_column(primary_key=True, autoincrement=True)
    processing_start: Mapped[datetime] 
    status_updated_at: Mapped[datetime] = mapped_column(
        server_default=func.now(),
        server_onupdate=func.now()
    )
    process_status: Mapped[str] 
