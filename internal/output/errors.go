package output

import (
	"encoding/json"
	"fmt"
	"io"
)

type Failure struct {
	Code          string            `json:"code"`
	Message       string            `json:"message"`
	HTTPStatus    *int              `json:"httpStatus"`
	BacklogErrors []json.RawMessage `json:"backlogErrors"`
	Creation      any               `json:"creation"`
}

func NewFailure(code, message string, status int) *Failure {
	result := &Failure{Code: code, Message: message, BacklogErrors: []json.RawMessage{}}
	if status != 0 {
		result.HTTPStatus = &status
	}
	return result
}

func (f *Failure) Error() string { return f.Message }

func (f *Failure) ExitCode() int {
	switch f.Code {
	case "input_error", "config_error":
		return 2
	case "cancelled":
		return 130
	default:
		return 1
	}
}

func WriteFailure(w io.Writer, failure *Failure, jsonMode bool, key string) error {
	if jsonMode {
		safe := *failure
		safe.Message = Redact(failure.Message, key)
		safe.BacklogErrors = make([]json.RawMessage, len(failure.BacklogErrors))
		for i, item := range failure.BacklogErrors {
			data, err := RedactJSON(item, key)
			if err != nil {
				return err
			}
			safe.BacklogErrors[i] = data
		}
		data, err := json.Marshal(struct {
			Error *Failure `json:"error"`
		}{&safe})
		if err != nil {
			return err
		}
		return write(w, append(data, '\n'))
	}
	_, err := fmt.Fprintln(w, Redact(SafeText(Redact(failure.Message, key)), key))
	return err
}

func write(w io.Writer, data []byte) error {
	n, err := w.Write(data)
	if err == nil && n != len(data) {
		return io.ErrShortWrite
	}
	return err
}
