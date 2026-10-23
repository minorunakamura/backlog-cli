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
	Creation      *Creation         `json:"creation"`
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
	if f.Code == "cancelled" {
		return 130
	}
	if f.Creation != nil && f.Creation.State == "created" {
		return 3
	}
	switch f.Code {
	case "input_error", "config_error":
		return 2
	default:
		return 1
	}
}

func WriteFailure(w io.Writer, failure *Failure, jsonMode bool, key string) error {
	message := failure.Message
	if failure.Creation != nil {
		switch failure.Creation.State {
		case "created":
			message += "; issue was created; do not create it again. If identity is missing, identification failed."
		case "unknown":
			message += "; creation could not be confirmed; do not recreate without checking for duplicates."
		}
	}
	if jsonMode {
		safe := *failure
		safe.Message = Redact(message, key)
		if failure.Creation != nil {
			creation := *failure.Creation
			if creation.IssueKey != nil {
				value := Redact(*creation.IssueKey, key)
				creation.IssueKey = &value
			}
			if creation.URL != nil {
				value := Redact(*creation.URL, key)
				creation.URL = &value
			}
			safe.Creation = &creation
		}
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
	if failure.Creation != nil {
		message += "\nCreation: " + failure.Creation.State
		if failure.Creation.IssueKey != nil {
			message += " " + *failure.Creation.IssueKey
		}
		if failure.Creation.URL != nil {
			message += " " + *failure.Creation.URL
		}
	}
	_, err := fmt.Fprintln(w, Redact(SafeText(Redact(message, key)), key))
	return err
}

func write(w io.Writer, data []byte) error {
	n, err := w.Write(data)
	if err == nil && n != len(data) {
		return io.ErrShortWrite
	}
	return err
}
