import onnxruntime as ort
ort.set_default_logger_severity(3)
print(ort.__version__, ort.get_available_providers())
ort.preload_dlls(cuda=True, cudnn=True, msvc=True)
so = ort.SessionOptions(); so.log_severity_level = 3
s = ort.InferenceSession('../models/sface.onnx', so, providers=['CUDAExecutionProvider', 'CPUExecutionProvider'])
print(s.get_providers())
for i in s.get_inputs(): print('in', i.name, i.shape, i.type)
for o in s.get_outputs(): print('out', o.name, o.shape)
y = ort.InferenceSession('../models/yunet.onnx', so, providers=['CPUExecutionProvider'])
for i in y.get_inputs(): print('yin', i.name, i.shape)
print([o.name + str(o.shape) for o in y.get_outputs()])
