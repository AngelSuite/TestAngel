app-name = TestAngel

# Generic useful words and phrases
ok = Ok
copy-ok = Copy & Ok
save = Save
discard = Discard
value = Value
nothing-open = Nothing is Open
delete = Delete

header-more = More...
header-new = New
header-open = Open...
header-save = Save
header-save-as = Save as...
header-close = Close
header-about = About { app-name }
acknowledgements-code-title = Code
acknowledgements-testing-title = Software Testing
acknowledgements-translations-title = Translations

kind-string = Text
kind-integer = Integer
kind-decimal = Decimal
kind-boolean = Boolean

tab-flows = Flows
tab-actions = Actions

variable-row-edit-param = Edit Parameter
variable-row-subtitle = { $kind }
variable-row-subtitle-with-value = { $kind }, { $source }: { $value }

drag-drop-here = Drop step here
move-up = Move up
move-down = Move down
delete-step = Delete step

source-literal = Literal value
source-data = From data spreadsheet column
source-from-step = From Step { $step }: { $name }
source-from-param = From Parameter: { $param }

# File types

filetype-all = All files
filetype-flow = { app-name } Flow file
filetype-action = { app-name } Action file
filetype-evp = Evidence Packages
filetype-csv = Comma Separated Values

# Flows

flow-header-add = Add step
flow-header-run = Run flow
flow-header-run-with-data = Run flow with data
flow-nothing-open-description = Open a flow or add a step to get started

flow-action-changed = Flow Actions Changed
flow-action-changed-message = The parameters in { $stepCount ->
        [one] step
       *[other] steps
    } { $steps } have changed so they has been reset.

flow-save-before = Save this Flow?
flow-save-before-message = This flow has been modified since it was last saved. Would you like to save it before continuing?
flow-saved = Flow saved
flow-error-saving = Error saving flow
flow-error-opening = Error opening flow

flow-save-open-error-io-error = I/O error: { $error }
flow-save-open-error-parsing-error = The flow file is corrupted: { $error }
flow-save-open-error-serializing-error = The flow could not be saved due to an internal serialisation error: { $error }
flow-save-open-error-flow-not-version-compatible = The flow you tried to load is not compatible with this version of { app-name }.
flow-save-open-error-missing-action = The action for step { $step } (with internal ID: { $error }) in this flow is missing.

flow-execution-running = Flow running...
flow-execution-failed = Flow failed.
flow-execution-failed-message = Flow failed at step { $step }: { $reason }.
flow-execution-failed-data = The data spreadsheet was invalid: { $reason }.
flow-execution-save-evidence-anyway = Save Evidence Anyway

flow-step-label = Step { $step }: { $name }

# Actions

action-header-add = Add step
action-header-add-to-open-flow = Add action to open flow
action-nothing-open-description = Open an action or add a step to get started

action-save-before = Save this action?
action-save-before-message = This action has been modified since it was last saved. Would you like to save it before continuing?
action-saved = Action saved
action-error-saving = Error saving action
action-error-opening = Error opening action

action-save-open-error-io-error = I/O error: { $error }
action-save-open-error-parsing-error = The action file is corrupted: { $error }
action-save-open-error-serializing-error = The action could not be saved due to an internal serialisation error: { $error }
action-save-open-error-action-not-version-compatible = The action you tried to load is not compatible with this version of { app-name }.
action-save-open-error-missing-instruction = The instruction with internal ID: { $error } in this action is missing.

action-default-group = Ungrouped
action-default-name = Untitled Action

action-params-new = New Input Parameter
action-params-name-placeholder = Parameter name
action-outputs-new = New Output
action-outputs-name-placeholder = Output name

action-step-set-comment = Set step comment
action-step-label = Step { $step }: { $name }
action-condition-run-always = Always runs
action-condition-run-condition = Runs if: { $cond }

# Execution

evidence-failed = Failed to save evidence
evidence-failed-message = Failed to save: { $reason }
evidence-save-title = Save evidence...
evidence-default-name = evidence.evp
