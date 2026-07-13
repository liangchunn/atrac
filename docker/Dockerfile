FROM i386/ubuntu:18.04

COPY at3tool /opt/at3tool/at3tool
COPY libatrac.so.1.2.0 /opt/at3tool/libatrac.so.1.2.0
RUN ln -sf libatrac.so.1.2.0 /opt/at3tool/libatrac.so.1
RUN chmod +x /opt/at3tool/at3tool

ENV LD_LIBRARY_PATH=/opt/at3tool

WORKDIR /data
ENTRYPOINT ["/opt/at3tool/at3tool"]
