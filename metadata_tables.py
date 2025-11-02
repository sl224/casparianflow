from sqlalchemy.orm import Mapped, mapped_column
from casp_sa_base import Base
from sqlalchemy import ForeignKey


# disable for prod -- for allowing jupyter cells

extend_existing = True

class FolderRecord(Base):
    __tablename__ = "folder"
    folder_id: Mapped[int] = mapped_column(primary_key=True, autoincrement=True)
    folder_path: Mapped[str] = mapped_column(unique=True)
    # remove for prod
    __table_args__ = {'extend_existing':extend_existing}

class FileRecord(Base):
    __tablename__ = "file"
    folder_id: Mapped[int] = mapped_column(ForeignKey("folder.folder_id"))
    file_id: Mapped[int] = mapped_column(primary_key=True, autoincrement=True)
    file_name: Mapped[str] 
    filesize_bytes: Mapped[int]
    # remove for prod
    __table_args__ = {'extend_existing': extend_existing}
