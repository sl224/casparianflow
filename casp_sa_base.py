from sqlalchemy.orm import DeclarativeBase
from sqlalchemy import inspect


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

    # Remove for prod
    __table_args__ = {"extend_existing": True}
